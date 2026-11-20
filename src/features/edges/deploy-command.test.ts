import { describe, expect, it } from 'vitest'
import { edgeDeployCommand } from './deploy-command'

describe('节点部署命令', () => {
  it('包含独立 Agent 所需的地址、凭据与独立数据目录', () => {
    const command = edgeDeployCommand({ controlPlaneURL: 'http://127.0.0.1:18083', token: "token'with-space", edgeId: 'node-123', shell: 'bash' })
    expect(command).toContain("export CONTROL_PLANE_URL='http://127.0.0.1:18083'")
    expect(command).toContain("export AGENT_TOKEN='token'\"'\"'with-space'")
    expect(command).toContain("export EDGE_DATA_DIR='./edge-data/node-123'")
    expect(command).toContain('./edge-agent')
    expect(command).not.toContain('BLOB_')
  })
  it('PowerShell 使用环境变量赋值并隐藏显示中的凭据', () => {
    const command = edgeDeployCommand({ controlPlaneURL: 'http://192.168.1.10:18083', token: 'secret-token', edgeId: 'node-123', shell: 'powershell', maskToken: true })
    expect(command).toContain("$env:CONTROL_PLANE_URL = 'http://192.168.1.10:18083'")
    expect(command).toContain("$env:AGENT_TOKEN = '********'")
    expect(command).toContain('.\\edge-agent.exe')
    expect(command).not.toContain('secret-token')
  })
})
