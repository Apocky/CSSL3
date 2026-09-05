import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const ROOT=path.dirname(fileURLToPath(import.meta.url));
const PUBLIC=path.resolve(ROOT,'../../public/codex-apockalypsis');
const temporary=fs.mkdtempSync(path.join(ROOT,'.repro-'));
const hash=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
function inventory(directory){
  const records=[];
  function walk(current){
    for(const entry of fs.readdirSync(current,{withFileTypes:true}).sort((a,b)=>a.name.localeCompare(b.name))){
      const file=path.join(current,entry.name);
      if(entry.isDirectory()) walk(file);
      else if(entry.isFile()) records.push({file:path.relative(directory,file).replaceAll('\\','/'),sha256:hash(fs.readFileSync(file))});
      else throw new Error('Publication inputs must be ordinary files and directories.');
    }
  }
  walk(directory);
  return records.sort((a,b)=>a.file<b.file?-1:a.file>b.file?1:0);
}
try{
  const before=inventory(PUBLIC);
  const build=spawnSync(process.execPath,[path.join(ROOT,'build.mjs'),'--out',temporary],{encoding:'utf8',windowsHide:true});
  if(build.status!==0) throw new Error(`Rebuild failed: ${build.stderr||build.stdout||build.error||'unknown error'}`);
  const after=inventory(temporary);
  const expected=new Map(before.map(record=>[record.file,record.sha256]));
  const actual=new Map(after.map(record=>[record.file,record.sha256]));
  const differences=[...new Set([...expected.keys(),...actual.keys()])].filter(file=>expected.get(file)!==actual.get(file));
  if(differences.length) throw new Error(`Rebuild differs: ${differences.join(', ')}`);
  if(JSON.stringify(before)!==JSON.stringify(inventory(PUBLIC))) throw new Error('Source publication changed during verification.');
  console.log(JSON.stringify({status:'PASS',files:before.length,treeSha256:hash(before.map(record=>`${record.sha256}  ${record.file}\n`).join(''))}));
}finally{
  const resolvedRoot=fs.realpathSync(ROOT);
  const resolvedTemporary=fs.realpathSync(temporary);
  const relative=path.relative(resolvedRoot,resolvedTemporary);
  if(path.isAbsolute(relative)||relative.startsWith('..')||relative.includes(path.sep)||!relative.startsWith('.repro-')) throw new Error('Refusing cleanup outside the owned temporary directory.');
  fs.rmSync(resolvedTemporary,{recursive:true,force:false});
}
