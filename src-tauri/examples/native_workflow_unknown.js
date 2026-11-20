void (async () => {
  const invoke=window.__TAURI_INTERNALS__.invoke
  const emit=(event,payload)=>invoke('plugin:event|emit',{event,payload})
  const request=path=>invoke('studio_request',{request:{method:'GET',path,body:null}})
  try {
    await emit('studio:layout-show')
    const workflows=await request('/api/v1/studio/workflows')
    const workflow=workflows.find(item=>item.type==='comfyui')
    const records=await request(`/api/v1/studio/workflow-executions?workflow_id=${workflow.id}`)
    const original=records.find(item=>item.status==='running' || item.status==='result_unknown')
    if (!original?.prompt_id) throw new Error('缺少需要核验原任务编号的执行')
    const end=Date.now()+30000
    let record
    do {
      record=await request(`/api/v1/studio/workflow-executions/${original.id}`)
      if (record.status==='result_unknown') break
      if (Date.now()>end) throw new Error('历史缺失的任务没有结束等待')
      await new Promise(resolve=>setTimeout(resolve,100))
    } while(true)
    if (record.prompt_id!==original.prompt_id || !record.error.includes('历史') || record.outputs.length) throw new Error('结果未知记录没有保留原任务信息')
    await emit('studio:layout-result',{success:true,execution:record.id,prompt:record.prompt_id,status:record.status,error:record.error,outputs:record.outputs.length})
  } catch(error) { await emit('studio:layout-result',{success:false,message:error.message}) }
})()
