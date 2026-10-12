/** Run AssemblyScript business Wasm locally, with in-process native adapter imports. */
import { readFileSync, mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { resolve, dirname } from 'node:path';
import { spawnSync } from 'node:child_process';
import { useAdapter } from '../../../adapter/index.mjs';
const here = dirname(fileURLToPath(import.meta.url));
const sdk = resolve(here, '../../..');
const out = resolve(here, '../target/wallet-adapter.wasm');
mkdirSync(dirname(out), {recursive:true});
const built = spawnSync(process.execPath, [resolve(sdk,'node_modules/assemblyscript/bin/asc.js'),resolve(here,'main.ts'),'-o',out,'--exportRuntime','--exportStart','_initialize'], {stdio:'inherit'});
if (built.status !== 0) throw new Error('AssemblyScript compilation failed; run npm ci in the SDK root');
const call = useAdapter();
let exports;
function stringAt(ptr) {
  const memory = exports.memory.buffer;
  const length = new DataView(memory).getUint32(ptr-4,true);
  return new TextDecoder('utf-16le').decode(new Uint8Array(memory,ptr,length));
}
const system = new Proxy({}, {get(_target, operation) { return (ptr,length,result) => {
  const input = new Uint8Array(exports.memory.buffer,ptr,length).slice();
  const output = call(operation.replaceAll('-','_'),input);
  const address = exports.cabi_realloc(0,0,1,output.length);
  new Uint8Array(exports.memory.buffer,address,output.length).set(output);
  const view = new DataView(exports.memory.buffer);
  view.setUint32(result,address,true);view.setUint32(result+4,output.length,true);
};}});
({instance:{exports}} = await WebAssembly.instantiate(readFileSync(out), {'mududb:api/system':system,env:{abort(message){throw new Error(stringAt(message));}}}));
exports._initialize();
function newString(text) {
  const p = exports.__pin(exports.__new(text.length*2,2));
  const view = new Uint16Array(exports.memory.buffer,p,text.length);
  for (let i=0;i<text.length;i++) view[i]=text.charCodeAt(i);
  return p;
}
function invoke(op,id=0n,to=0n,amount=0n,name='',email='') {
  const n=newString(name),e=newString(email);
  try{return exports.execute(op,id,to,amount,n,e);}finally{exports.__unpin(n);exports.__unpin(e);}
}
const [operation,...args]=process.argv.slice(2);
switch(operation) {
 case 'init':invoke(0);break;
 case 'create':console.log(String(invoke(1,BigInt(args[0]),0n,0n,args[1],args[2])));break;
 case 'deposit':case 'withdraw':console.log(String(invoke(operation==='deposit'?2:3,BigInt(args[0]),0n,BigInt(args[1]))));break;
 case 'transfer':console.log(String(invoke(4,BigInt(args[0]),BigInt(args[1]),BigInt(args[2]))));break;
 case 'balance':console.log(String(invoke(5,BigInt(args[0]))));break;
 case 'scenario':
  invoke(1,1001n,0n,0n,'Alice','alice@example.com');invoke(1,1002n,0n,0n,'Bob','bob@example.com');
  invoke(2,1001n,0n,1000n);invoke(3,1001n,0n,100n);invoke(4,1001n,1002n,250n);
  if(invoke(5,1001n)!==650n||invoke(5,1002n)!==250n)throw new Error('Balance mismatch');
  console.log('{"alice":650,"bob":250}');break;
 default:throw new Error('Usage: init | scenario | create ID NAME EMAIL | deposit ID AMOUNT | withdraw ID AMOUNT | transfer FROM TO AMOUNT | balance ID');
}
