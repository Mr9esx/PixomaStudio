export type AgentShell = 'bash' | 'powershell'

export function edgeDeployCommand(input: {
  controlPlaneURL: string
  token: string
  edgeId: string
  shell: AgentShell
  maskToken?: boolean
}): string {
  const values = {
    CONTROL_PLANE_URL: input.controlPlaneURL,
    AGENT_TOKEN: input.maskToken ? '********' : input.token,
    EDGE_ID: input.edgeId,
    EDGE_DATA_DIR: `./edge-data/${input.edgeId}`,
  }
  const quote = (value: string) => input.shell === 'powershell'
    ? `'${value.replaceAll("'", "''")}'`
    : `'${value.replaceAll("'", "'\"'\"'")}'`
  return [...Object.entries(values).map(([key, value]) => input.shell === 'powershell'
    ? `$env:${key} = ${quote(value)}`
    : `export ${key}=${quote(value)}`), input.shell === 'powershell' ? '.\\edge-agent.exe' : './edge-agent'].join('\n')
}
