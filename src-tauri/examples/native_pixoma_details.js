void (async () => {
  const invoke = window.__TAURI_INTERNALS__.invoke
  const emit = (event,payload) => invoke('plugin:event|emit',{event,payload})
  const request = (method,path,body=null) => invoke('studio_request',{request:{method,path,body}})
  const assert = (condition,message) => { if (!condition) throw new Error(message) }
  const pause = () => new Promise(resolve => setTimeout(resolve,80))
  const wait = async (condition,message) => { const end=Date.now()+25000; while (!await condition()) { if (Date.now()>end) throw new Error(message); await pause() } await pause() }
  const button = (label,parent=document) => [...parent.querySelectorAll('button')].find(item => item.getClientRects().length && item.textContent.trim()===label)
  const click = label => { const target=button(label); assert(target,`操作缺失：${label}`); target.click() }
  const fill = (input,value) => { assert(input,'输入框缺失'); Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(input,value); input.dispatchEvent(new Event('input',{bubbles:true})) }
  let stage='计算节点详情'
  try {
    await emit('studio:layout-show')
    const nodes=await request('GET','/api/v1/studio/compute-nodes')
    const node=nodes.find(item => item.location==='local')
    const workflows=await request('GET','/api/v1/studio/workflows')
    let workflow=workflows.find(item => item.type==='comfyui')
    assert(node && workflow,'真实验收节点和工作流缺失')
    const definition=structuredClone(workflow.draft)
    definition.input_schema.properties.prompt={...definition.input_schema.properties.prompt,minLength:5,default:definition.execution.workflow['40'].inputs.text}
    workflow=await request('PATCH',`/api/v1/studio/workflows/${workflow.id}`,{expected_revision:workflow.revision,draft:definition})
    await wait(() => button('设置'),'设置入口缺失')
    click('设置'); await wait(() => button('计算节点'),'设置菜单缺失'); click('计算节点')
    await wait(() => [...document.querySelectorAll('#edge-list button')].some(item => item.textContent.includes(node.name)),'节点列表缺失'); [...document.querySelectorAll('#edge-list button')].find(item => item.textContent.includes(node.name)).click()
    await wait(() => document.querySelector('[data-testid="edge-detail"]'),'节点详情缺失')
    const detail=document.querySelector('[data-testid="edge-detail"]')
    assert(detail.querySelector('h2').textContent===node.name,'节点详情标题不一致')
    assert(detail.innerText.includes('节点 ID') && detail.innerText.includes('创建时间') && detail.innerText.includes('启动时间'),'节点元信息缺失')
    const specs=[...detail.querySelectorAll('section > div')].filter(item => item.className.includes('border-border/70'))
    assert(specs.length===4 && specs[2].textContent.includes('核数') && specs[3].textContent.includes('内存'),'硬件规格顺序不一致')
    await wait(() => detail.querySelector('[data-slot="card"] .md\\:grid-cols-3'),'三列统计没有显示')
    assert(button('编辑',detail) && button('部署命令',detail),'节点原有操作缺失')
    click('自定义'); await wait(() => document.querySelector('#node-monitor-from'),'自定义监控范围缺失')
    const from=document.querySelector('#node-monitor-from')
    const to=document.querySelector('#node-monitor-to')
    assert(from.value && to.value && Math.abs(from.getBoundingClientRect().top-to.getBoundingClientRect().top)<2,'监控时间输入布局不一致')
    click('应用'); await wait(() => !document.querySelector('#node-monitor-from'),'自定义监控范围没有应用')
    for (const width of [1440,1024,760]) {
      await emit('studio:layout-size',[width,900]); await wait(() => innerWidth===width,'节点窗口尺寸没有更新')
      const main=document.querySelector('[data-slot="studio-page"]')
      assert(main.scrollWidth<=main.clientWidth,'节点详情超出窗口')
    }
    await emit('studio:layout-size',[1440,900]); await wait(() => innerWidth===1440,'窗口尺寸没有恢复')
    await emit('studio:layout-progress',{stage})
    stage='工作流详情'
    click('工作流'); await wait(() => document.querySelector(`a[href="/studio/workflows/${workflow.id}"]`),'工作流列表缺失')
    document.querySelector(`a[href="/studio/workflows/${workflow.id}"]`).click()
    await wait(() => document.querySelector('[data-testid="case-detail-panel"]'),'工作流详情缺失')
    assert(document.querySelector('[role="tab"][data-state="active"]').textContent==='工作流配置','工作流默认页面不一致')
    assert(button('编辑') && button('编辑工作流配置') && button('编辑输入') && button('编辑输出'),'原工作流编辑操作缺失')
    await wait(() => document.querySelector('[data-testid="workflow-graph-viewer"]'),'原工作流查看器缺失')
    click('编辑')
    await wait(() => document.querySelector('#case-name'),'基础信息编辑缺失')
    assert(!document.querySelector('[data-testid="case-section-workflow-import"]'),'基础信息编辑包含工作流编辑')
    const name=workflow.name+' 信息验证'
    fill(document.querySelector('#case-name'),name)
    document.querySelector('#workflow-editor').requestSubmit()
    await wait(() => document.querySelector('[data-testid="case-detail-panel"]'),'基础信息没有保存')
    workflow=await request('GET',`/api/v1/studio/workflows/${workflow.id}`)
    assert(workflow.name===name && JSON.stringify(workflow.draft.input_schema)===JSON.stringify(definition.input_schema),'基础信息保存改变输入约束')
    assert(JSON.stringify(workflow.draft.execution)===JSON.stringify(definition.execution),'基础信息保存改变工作流绑定')
    click('编辑输入')
    await wait(() => document.querySelector('[data-testid="input-fields-table"] input:not([disabled])'),'页内输入编辑缺失')
    assert(!document.querySelector('[role="dialog"]'),'输入编辑打开模态窗口')
    const description=document.querySelector('[data-testid="input-fields-table"] input[aria-label="描述"]')
    fill(description,'生成要求 · 输入验证')
    click('保存')
    await wait(() => button('编辑输入') && !button('编辑输入').disabled,'输入编辑没有保存')
    workflow=await request('GET',`/api/v1/studio/workflows/${workflow.id}`)
    assert(workflow.draft.inputs[0].description==='生成要求 · 输入验证','输入说明没有保存')
    assert(workflow.draft.input_schema.properties.prompt.minLength===5 && workflow.draft.input_schema.properties.prompt.default===definition.input_schema.properties.prompt.default,'输入说明编辑改变输入约束')
    click('编辑输出'); await wait(() => document.querySelector('[data-testid="output-fields-table"] input:not([disabled])'),'页内输出编辑缺失'); click('取消')
    stage='编辑草稿与节点设置往返'
    click('编辑工作流配置'); await wait(() => document.querySelector('[data-testid="case-section-workflow-import"]'),'工作流编辑缺失')
    assert(!document.querySelector('#case-name'),'工作流编辑包含基础信息表单')
    const key=document.querySelector('[data-testid="input-fields-table"] input[aria-label="字段名"]')
    fill(key,'prompt_review')
    click('管理节点'); await wait(() => button('返回工作流'),'节点设置没有返回入口'); click('返回工作流')
    await wait(() => document.querySelector('[data-testid="input-fields-table"] input[aria-label="字段名"]'),'工作流草稿没有恢复')
    assert(document.querySelector('[data-testid="input-fields-table"] input[aria-label="字段名"]').value==='prompt_review','节点设置往返丢失草稿')
    click('取消'); await wait(() => document.querySelector('[role="alertdialog"]'),'取消没有检查未保存内容'); click('放弃修改')
    await wait(() => document.querySelector('[data-testid="case-detail-panel"]'),'取消没有返回工作流详情')
    const widths=[]
    for (const width of [1440,1024,760]) {
      await emit('studio:layout-size',[width,900]); await wait(() => innerWidth===width,'窗口尺寸没有更新')
      const main=document.querySelector('[data-slot="studio-page"]')
      assert(main.scrollWidth<=main.clientWidth,'工作流详情超出窗口')
      widths.push({width,scroll:main.scrollWidth,client:main.clientWidth})
    }
    await emit('studio:layout-result',{success:true,node:node.id,workflow:workflow.id,metadataPreserved:true,inlineFieldEditing:true,editorRestored:true,widths})
  } catch(error) { await emit('studio:layout-result',{success:false,stage,message:error.message,pageText:document.body.innerText.slice(-1800)}) }
})()
