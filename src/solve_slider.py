import json, sys
import os
import shutil
from datetime import datetime, timezone
from pathlib import Path
import cv2
import numpy as np

bg_path=Path(sys.argv[1])
piece_path=Path(sys.argv[2])
bg=cv2.imread(str(bg_path),cv2.IMREAD_UNCHANGED)
piece=cv2.imread(str(piece_path),cv2.IMREAD_UNCHANGED)
if bg is None or piece is None or piece.shape[2]<4:
    raise SystemExit('RGBA background/piece images required')
mask=(piece[:,:,3]>128).astype(np.uint8)
boundary=cv2.morphologyEx(mask,cv2.MORPH_GRADIENT,np.ones((3,3),np.uint8))>0
edges=cv2.Canny(cv2.cvtColor(bg[:,:,:3],cv2.COLOR_BGR2GRAY),50,150)
gray=cv2.cvtColor(bg[:,:,:3],cv2.COLOR_BGR2GRAY)
hsv=cv2.cvtColor(bg[:,:,:3],cv2.COLOR_BGR2HSV)
gray_f=gray.astype(np.float32)
gradient=np.abs(np.roll(gray_f,-1,axis=1)-np.roll(gray_f,1,axis=1)) + np.abs(np.roll(gray_f,-1,axis=0)-np.roll(gray_f,1,axis=0))
edge_map=cv2.Canny(gray,50,150)
edge_distance=cv2.distanceTransform((edge_map==0).astype(np.uint8),cv2.DIST_L2,3)
interior=cv2.erode(mask,np.ones((5,5),np.uint8))>0
ys,xs=np.where(boundary)
scores=[]
min_offset=int(os.environ.get('SOLVER_MIN_OFFSET','120'))
max_offset=bg.shape[1]-piece.shape[1]
legacy_weight=0.35
gradient_weight=0.50
distance_weight=0.15
for x in range(min_offset,max_offset+1):
    vals=edges[ys,xs+x]
    patch=bg[:,:,:3][:,x:x+piece.shape[1]]
    edge_score=float(vals.mean())
    brightness=float(gray[:,x:x+piece.shape[1]][interior].mean())
    saturation=float(hsv[:,x:x+piece.shape[1],1][interior].mean())
    variation=float(patch[interior].std())
    gradient_score=float(np.max(np.stack([
        gradient[np.clip(ys+dy,0,gradient.shape[0]-1),np.clip(xs+x+dx,0,gradient.shape[1]-1)]
        for dx in (-1,0,1) for dy in (-1,0,1)
    ],axis=0),axis=0).mean())
    edge_distance_score=float(edge_distance[ys,np.clip(xs+x,0,edge_distance.shape[1]-1)].mean())
    # The missing slot is rendered as a bright, desaturated overlay. Combining
    # that signal with the alpha-boundary edge score avoids false matches on
    # naturally bright objects (for example a sun or lamp).
    legacy_score=edge_score+.8*brightness-.5*saturation-.3*variation
    scores.append((legacy_score,x,int(np.count_nonzero(vals)),edge_score,brightness,saturation,variation,gradient_score,edge_distance_score))
scores.sort(reverse=True)
legacy_min=min(s[0] for s in scores); legacy_max=max(s[0] for s in scores)
grad_min=min(s[7] for s in scores); grad_max=max(s[7] for s in scores)
distance_min=min(s[8] for s in scores); distance_max=max(s[8] for s in scores)
ranked=[]
for s in scores:
    legacy_norm=(s[0]-legacy_min)/(legacy_max-legacy_min or 1)
    gradient_norm=(s[7]-grad_min)/(grad_max-grad_min or 1)
    distance_norm=1-(s[8]-distance_min)/(distance_max-distance_min or 1)
    ranked.append((legacy_weight*legacy_norm+gradient_weight*gradient_norm+distance_weight*distance_norm,s))
ranked.sort(key=lambda item:(item[0],item[1][0]),reverse=True)
best_combined,best=ranked[0]
template_offset=None
template_score=None
try:
    template_result=cv2.matchTemplate(
        cv2.cvtColor(bg[:,:,:3],cv2.COLOR_BGR2GRAY),
        cv2.cvtColor(piece[:,:,:3],cv2.COLOR_BGR2GRAY),
        cv2.TM_CCORR_NORMED,
        mask=piece[:,:,3],
    )
    _,template_score,_,template_loc=cv2.minMaxLoc(template_result)
    template_offset=int(template_loc[0])
except cv2.error:
    pass

delta=None if template_offset is None else abs(best[1]-template_offset)
out={'algorithm':'boundary_fusion_v1','weights':{'legacy':legacy_weight,'alphaGradient':gradient_weight,'edgeDistance':distance_weight},'sourceOffset':best[1],'score':best_combined,'legacyScore':best[0],'gradientScore':best[7],'edgeDistanceScore':best[8],'templateOffset':template_offset,'templateScore':template_score,'diagnosticDelta':delta,'minOffset':min_offset,'maxOffset':max_offset,'edgeHits':best[2],'edgeScore':best[3],'brightness':best[4],'saturation':best[5],'variation':best[6],'backgroundWidth':bg.shape[1],'pieceWidth':piece.shape[1],'topCandidates':[{'x':s[1],'combinedScore':combined,'legacyScore':s[0],'gradientScore':s[7],'edgeDistanceScore':s[8],'edgeHits':s[2],'edgeScore':s[3]} for combined,s in ranked[:8]]}

anomaly_dir=os.environ.get('SOLVER_ANOMALY_DIR')
capture_dir=os.environ.get('SOLVER_CAPTURE_DIR')
if capture_dir:
    stamp=datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%S%fZ')
    target=Path(capture_dir)/stamp
    target.mkdir(parents=True,exist_ok=True)
    shutil.copy2(bg_path,target/'background.png')
    shutil.copy2(piece_path,target/'piece.png')
    (target/'solver.json').write_text(json.dumps(out,ensure_ascii=False,indent=2),encoding='utf-8')
    out['captureDir']=str(target)
threshold=float(os.environ.get('SOLVER_ANOMALY_THRESHOLD','30'))
if anomaly_dir and delta is not None and delta>threshold:
    stamp=datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%S%fZ')
    target=Path(anomaly_dir)/stamp
    target.mkdir(parents=True,exist_ok=True)
    shutil.copy2(bg_path,target/'background.png')
    shutil.copy2(piece_path,target/'piece.png')
    (target/'diagnostic.json').write_text(json.dumps(out,ensure_ascii=False,indent=2),encoding='utf-8')
    out['anomalyArchive']=str(target)
print(json.dumps(out,ensure_ascii=False))
