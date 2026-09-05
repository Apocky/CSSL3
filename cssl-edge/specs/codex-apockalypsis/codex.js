'use strict';
// § public.local.interaction ; no.storage/network/eval
const $$=(s)=>Array.from(document.querySelectorAll(s));
let category='all';
function filterLibrary(){
 const q=(document.querySelector('#library-search')?.value??'').trim().toLowerCase();let n=0;
 $$('[data-document]').forEach(el=>{const show=(category==='all'||el.dataset.category===category)&&el.dataset.search.includes(q);el.hidden=!show;if(show)n++;});
 const c=document.querySelector('#library-count');if(c)c.textContent=n+' '+(n===1?'text':'texts');
 const e=document.querySelector('#library-empty');if(e)e.hidden=n!==0;
}
$$('[data-filter]').forEach(b=>b.addEventListener('click',()=>{category=b.dataset.filter;$$('[data-filter]').forEach(x=>x.setAttribute('aria-pressed',String(x===b)));filterLibrary();}));
document.querySelector('#library-search')?.addEventListener('input',filterLibrary);
document.querySelector('#source-search')?.addEventListener('input',e=>{const q=e.target.value.toLowerCase().trim();let n=0;$$('[data-source]').forEach(r=>{r.hidden=!r.dataset.search.includes(q);if(!r.hidden)n++;});document.querySelector('#source-count').textContent=n+' records';document.querySelector('#source-empty').hidden=n!==0;});
document.querySelector('#reader-theme')?.addEventListener('click',e=>{const paper=document.body.classList.toggle('paper');e.currentTarget.setAttribute('aria-pressed',String(paper));e.currentTarget.textContent=paper?'Night view':'Paper view';});
document.querySelector('#reader-size')?.addEventListener('click',e=>{const large=document.body.classList.toggle('large-type');e.currentTarget.setAttribute('aria-pressed',String(large));e.currentTarget.textContent=large?'Standard type':'Larger type';});
