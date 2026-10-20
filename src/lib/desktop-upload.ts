export async function createDesktopUpload(
  file: File,
  sessionId: string | undefined,
  projectId: string,
  requestId: string,
  purpose?: 'expert_avatar',
) {
  if (file.size > 256 * 1024 * 1024) throw new Error('文件超过 256 MiB')
  if (purpose === 'expert_avatar' && file.size > 5 * 1024 * 1024) throw new Error('选择 5 MiB 以内的头像图片')
  if (!file.name.trim() || !requestId.trim()) throw new Error('文件名称和请求标识不能为空')
  return {
    bytes: await file.arrayBuffer(),
    metadata: encodeURIComponent(JSON.stringify({
      file_name: file.name,
      ...(sessionId ? { session_id: sessionId } : { project_id: projectId }),
      request_id: requestId,
      ...(purpose ? { purpose } : {}),
      ...(Number.isFinite(file.lastModified) && file.lastModified > 0
        ? { source_modified_at: new Date(file.lastModified).toISOString() }
        : {}),
    })),
  }
}
