void (async () => {
  const invoke=window.__TAURI_INTERNALS__.invoke
  const emit=(event,payload)=>invoke('plugin:event|emit',{event,payload})
  const request=(method,path,body=null)=>invoke('studio_request',{request:{method,path,body}})
  const wait=async (condition,message)=>{ const end=Date.now()+90000; while (!await condition()) {if(Date.now()>end) throw new Error(message); await new Promise(resolve=>setTimeout(resolve,100))} }
  const button=label=>[...document.querySelectorAll('button')].find(item=>item.getClientRects().length && item.textContent.trim()===label)
  try {
    await emit('studio:layout-show')
    const workflow=(await request('GET','/api/v1/studio/workflows')).find(item=>item.type==='comfyui')
    const path=`/api/v1/studio/workflow-executions?workflow_id=${workflow.id}`
    const previous=(await request('GET',path)).length
    await wait(()=>button('工作流'),'工作流入口缺失'); button('工作流').click()
    await wait(()=>document.querySelector(`a[href="/studio/workflows/${workflow.id}"]`),'工作流列表缺失'); document.querySelector(`a[href="/studio/workflows/${workflow.id}"]`).click()
    await wait(()=>button('运行草稿'),'工作流详情缺失'); button('运行草稿').click()
    await wait(()=>document.querySelector('#workflow-test-prompt'),'工作流表单缺失')
    const prompt=document.querySelector('#workflow-test-prompt')
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value').set.call(prompt,`清晨茶园的自然风光，阳光照射茶叶，电影摄影。取消交互验证 ${Date.now()}`)
    prompt.dispatchEvent(new Event('input',{bubbles:true}))
    await new Promise(resolve=>setTimeout(resolve,100)); button('运行工作流').click()
    let execution
    await wait(async()=>{const records=await request('GET',path); if(records.length<=previous)return false; execution=records[0]; return Boolean(execution.prompt_id)},'实际 ComfyUI 提交没有确认')
    await emit('studio:layout-focus')
    await wait(()=>button('取消执行'),'取消执行按钮缺失'); button('取消执行').click()
    await wait(async()=>{execution=await request('GET',`/api/v1/studio/workflow-executions/${execution.id}`); if(execution.status==='failed'||execution.status==='result_unknown')throw new Error(execution.error); return execution.status==='cancelled'},'定向取消没有确认')
    if(!execution.cancel_requested||execution.outputs.length)throw new Error('取消后的执行记录无效')
    await emit('studio:layout-result',{success:true,execution:execution.id,prompt:execution.prompt_id,status:execution.status,error:execution.error,outputs:execution.outputs.length})
  } catch(error) {await emit('studio:layout-result',{success:false,message:error.message,pageText:document.body.innerText.slice(-1200)})}
})()
