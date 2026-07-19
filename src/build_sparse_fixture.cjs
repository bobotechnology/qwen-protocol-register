// build_sparse_fixture.cjs
// Creates a device fixture that matches the browser's sparse field pattern.
// The browser (feilin106) leaves 101/133 fields empty in the deviceToken plaintext.
// Our previous fixture filled all 122 fields, making the encrypted deviceToken
// 3x larger than expected (2496 vs 856 chars), triggering F025 detection.
const fs = require('fs');
const path = require('path');

const SEED_DIR = process.env.FIXTURES_DIR || path.join(__dirname, '..', 'fixtures');
const OUT = process.env.PIPELINE_OUT || SEED_DIR;
const sparseSource = path.join(SEED_DIR, 'sparse_ip_plaintext.txt');
const initFile = path.join(OUT, 'fresh_init_response.json');
const outputFile = path.join(OUT, 'adapted_device_object.json');

const fields = fs.readFileSync(sparseSource, 'utf8').split('#');
const init = JSON.parse(fs.readFileSync(initFile, 'utf8'));

if (fields.length !== 133) throw new Error(`expected 133 fields, got ${fields.length}`);

const now = Date.now();
const deviceConfig = require('./decrypt_device_config.cjs').decryptDeviceConfig(init.DeviceConfig);
// Update dynamic fields using the BROWSER's field mapping
fields[72] = String(now + 500);   // timestamp
fields[74] = String(now + 1000);  // timestamp  
fields[77] = init.CertifyId;      // certifyId (at k77, matching browser)
fields[87] = String(now);         // timestamp
fields[42] = deviceConfig.ip;     // server-observed IP must match this session
const obj = Object.fromEntries(fields.map((v, i) => [`k${i}`, v]));
fs.writeFileSync(outputFile, JSON.stringify(obj));
console.log(JSON.stringify({
  output: outputFile,
  fields: fields.length,
  nonEmpty: fields.filter(f => f !== '').length,
  certifyId: init.CertifyId,
  totalChars: fields.join('#').length
}));


