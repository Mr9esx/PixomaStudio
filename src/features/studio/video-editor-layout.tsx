import { useEffect, useRef, useState, type ReactNode } from 'react'
import { useDefaultLayout, useGroupRef } from 'react-resizable-panels'
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from '@/components/ui/resizable'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Button } from '@/components/ui/button'
import { RotateCcw } from 'lucide-react'

type Props = {
  assets: ReactNode
  script: ReactNode
  scriptRequest?: string
  preview: ReactNode
  timeline: ReactNode
  inspector: ReactNode
  libraryOpen: boolean
  previewTools?: ReactNode
  onCloseInspector: () => void
  onLibraryChange: (open: boolean) => void
  onPreviewVisibilityChange: (visible: boolean) => void
  onResetLayout: () => void
}

export function VideoEditorLayout({ assets, script, scriptRequest, preview, previewTools, timeline, inspector, libraryOpen, onCloseInspector, onLibraryChange, onPreviewVisibilityChange, onResetLayout }: Props) {
  const root = useRef<HTMLDivElement>(null)
  const [compact, setCompact] = useState(false)
  const [stacked, setStacked] = useState(false)
  const [scriptVisible, setScriptVisible] = useState(() => localStorage.getItem('video-editor-script-visible') !== 'false')
  useEffect(() => { if (scriptRequest) { setScriptVisible(true); onLibraryChange(false); onCloseInspector() } }, [scriptRequest])
  const verticalGroup = useGroupRef()
  const horizontalGroup = useGroupRef()
  const previewGroup = useGroupRef()
  const vertical = useDefaultLayout({ id: 'video-editor-vertical', panelIds: ['workspace', 'timeline'], onlySaveAfterUserInteractions: true })
  const horizontal = useDefaultLayout({ id: `video-editor-content-${Boolean(inspector)}`, panelIds: ['content', ...(inspector ? ['inspector'] : [])], onlySaveAfterUserInteractions: true })
  const previewLayout = useDefaultLayout({ id: `video-editor-preview-${stacked}-${scriptVisible}`, panelIds: [...(scriptVisible ? ['script'] : []), 'preview'], onlySaveAfterUserInteractions: true })
  useEffect(() => localStorage.setItem('video-editor-script-visible', String(scriptVisible)), [scriptVisible])
  const previewVisible = !libraryOpen && !(compact && inspector)
  useEffect(() => onPreviewVisibilityChange(previewVisible), [previewVisible, onPreviewVisibilityChange])
  useEffect(() => {
    const observer = new ResizeObserver(entries => { const width = entries[0].contentRect.width; setCompact(width < 900); setStacked(width < 520) })
    observer.observe(root.current!)
    return () => observer.disconnect()
  }, [])
  function resetVerticalLayout() {
    const layout = verticalGroup.current?.setLayout({ workspace: 60, timeline: 40 })
    if (layout) vertical.onLayoutChanged(layout, { isUserInteraction: true })
  }
  function resetHorizontalLayout() {
    const width = root.current?.clientWidth ?? 1
    const inspectorSize = inspector ? 280 / width * 100 : 0
    const layout = horizontalGroup.current?.setLayout({ content: 100 - inspectorSize, ...(inspector ? { inspector: inspectorSize } : {}) })
    if (layout) horizontal.onLayoutChanged(layout, { isUserInteraction: true })
  }
  function resetPreviewLayout() {
    const width = root.current?.querySelector<HTMLElement>('.sve-preview-workspace')?.clientWidth || root.current?.clientWidth || 1
    const scriptSize = scriptVisible ? stacked ? 42 : Math.min(40, 280 / width * 100) : 0
    const layout = previewGroup.current?.setLayout({ ...(scriptVisible ? { script: scriptSize } : {}), preview: 100 - scriptSize })
    if (layout) previewLayout.onLayoutChanged(layout, { isUserInteraction: true })
  }
  function resetLayout() { resetVerticalLayout(); resetHorizontalLayout(); resetPreviewLayout() }
  const content = <Tabs className='sve-workspace-tabs' value={compact && inspector ? 'inspector' : libraryOpen ? 'assets' : 'preview'} onValueChange={value => {
    if (compact && value !== 'inspector') onCloseInspector()
    if (value !== 'inspector') onLibraryChange(value === 'assets')
  }}>
    <div className='sve-preview-header'><TabsList aria-label='编辑器工作区域'><TabsTrigger value='preview'>预览</TabsTrigger><TabsTrigger value='assets'>项目资产</TabsTrigger>{compact && inspector ? <TabsTrigger value='inspector'>对象编辑</TabsTrigger> : null}</TabsList><div className='sve-actions'>{previewVisible ? <Button size='sm' variant='ghost' aria-label={scriptVisible ? '收起脚本区域' : '显示脚本区域'} aria-pressed={scriptVisible} onClick={() => setScriptVisible(value => !value)}>{scriptVisible ? '收起脚本' : '显示脚本'}</Button> : null}{previewVisible ? previewTools : null}<Button size='icon-sm' variant='ghost' aria-label='恢复编辑器布局' onClick={() => { resetLayout(); onResetLayout() }}><RotateCcw /></Button></div></div>
    <TabsContent value='preview' forceMount className='sve-workspace-content data-[state=inactive]:hidden'>
      <ResizablePanelGroup groupRef={previewGroup} className='sve-preview-workspace' orientation={stacked ? 'vertical' : 'horizontal'} defaultLayout={previewLayout.defaultLayout} onLayoutChanged={previewLayout.onLayoutChanged}>
        {scriptVisible ? <><ResizablePanel id='script' defaultSize={stacked ? '42%' : 280} minSize={stacked ? 80 : 160} maxSize={stacked ? '65%' : 420}>{script}</ResizablePanel>
        <ResizableHandle aria-label={stacked ? '调整脚本区域高度' : '调整脚本区域宽度'} disableDoubleClick onDoubleClick={resetPreviewLayout} /></> : null}
        <ResizablePanel id='preview' minSize={stacked ? 100 : 180}>{preview}</ResizablePanel>
      </ResizablePanelGroup>
    </TabsContent>
    <TabsContent value='assets' forceMount className='sve-workspace-content data-[state=inactive]:hidden'>{assets}</TabsContent>
    {compact && inspector ? <TabsContent value='inspector' className='sve-workspace-content'>{inspector}</TabsContent> : null}
  </Tabs>
  return <div ref={root} className='sve-layout' data-compact={compact}>
    <ResizablePanelGroup groupRef={verticalGroup} orientation='vertical' defaultLayout={vertical.defaultLayout} onLayoutChanged={vertical.onLayoutChanged}>
      <ResizablePanel id='workspace' defaultSize='60%' minSize={stacked && scriptVisible ? 220 : 120}>
        {compact ? content : <ResizablePanelGroup groupRef={horizontalGroup} orientation='horizontal' defaultLayout={horizontal.defaultLayout} onLayoutChanged={horizontal.onLayoutChanged}>
          <ResizablePanel id='content' minSize={180}>{content}</ResizablePanel>
          {inspector ? <><ResizableHandle aria-label='调整属性区域宽度' disableDoubleClick onDoubleClick={resetHorizontalLayout} /><ResizablePanel id='inspector' defaultSize={280} minSize={240} maxSize={480}>{inspector}</ResizablePanel></> : null}
        </ResizablePanelGroup>}
      </ResizablePanel>
      <ResizableHandle aria-label='调整预览与时间轴高度' disableDoubleClick onDoubleClick={resetVerticalLayout} />
      <ResizablePanel id='timeline' defaultSize='40%' minSize={160}><div className='sve-timeline-pane'>{timeline}</div></ResizablePanel>
    </ResizablePanelGroup>
  </div>
}
