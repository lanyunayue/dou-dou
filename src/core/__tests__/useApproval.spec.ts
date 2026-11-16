import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const { gwFetchMock, markGwFailureMock } = vi.hoisted(() => ({
  gwFetchMock: vi.fn(),
  markGwFailureMock: vi.fn(),
}))

vi.mock('../../adapters/useGateway', () => ({
  gwFetch: gwFetchMock,
  gwError: { value: null as { message: string } | null },
  markGwFailure: markGwFailureMock,
}))

const T0 = 1_000_000

function okResp(items: unknown[]) {
  return { ok: true, status: 200, json: async () => ({ items }) }
}

async function loadModule() {
  return await import('../useApproval')
}

const alertMock = vi.fn()

beforeEach(() => {
  vi.useFakeTimers()
  vi.setSystemTime(T0)
  vi.clearAllMocks()
  vi.stubGlobal('alert', alertMock)
})

afterEach(() => {
  vi.unstubAllGlobals()
  vi.useRealTimers()
})

describe('useApproval · A4 断连置顶', () => {
  it('空态：视图为空、计数为 0、未失联', async () => {
    const m = await loadModule()
    expect(m.pendingTasksView.value).toEqual([])
    expect(m.pendingCount.value).toBe(0)
    expect(m.gwDisconnected.value).toBe(false)
  })

  it('会话启动前挂起的任务 → orphaned 标注', async () => {
    vi.setSystemTime(T0 + 5_000)
    const m = await loadModule()
    gwFetchMock.mockResolvedValueOnce(okResp([
      { id: 'old', created_ms: T0 - 1_000, status: 'pending_approval' },
    ]))
    await m.loadTasks()
    const view = m.pendingTasksView.value
    expect(view).toHaveLength(1)
    expect(view[0].orphaned).toBe(true)
  })

  it('失联窗口：开始前挂起 → orphaned；失联期间新建 → 不标注；恢复后标记保留', async () => {
    const m = await loadModule()
    vi.setSystemTime(T0 + 5_000)
    gwFetchMock.mockRejectedValueOnce(new Error('gateway offline'))
    await m.loadTasks()
    expect(m.gwDisconnected.value).toBe(true)
    expect(m.pendingTasks.value).toEqual([])
    expect(markGwFailureMock).toHaveBeenCalledWith('加载待审任务失败：网关不可用')
    const outageStart = T0 + 5_000

    vi.setSystemTime(T0 + 15_000)
    gwFetchMock.mockResolvedValueOnce(okResp([
      { id: 'pre-outage', created_ms: outageStart - 1_000, status: 'pending_approval' },
      { id: 'in-outage', created_ms: outageStart + 5_000, status: 'pending_approval' },
    ]))
    await m.loadTasks()
    expect(m.gwDisconnected.value).toBe(false)
    const view = m.pendingTasksView.value
    expect(view.find(v => v.id === 'pre-outage')!.orphaned).toBe(true)
    expect(view.find(v => v.id === 'in-outage')!.orphaned).toBe(false)
  })

  it('排序：断连任务置顶，普通任务按创建时间倒序', async () => {
    const m = await loadModule()
    gwFetchMock.mockResolvedValueOnce(okResp([
      { id: 'normal-1', created_ms: T0 + 2_100, status: 'pending_approval' },
      { id: 'orphan', created_ms: T0 - 500, status: 'pending_approval' },
      { id: 'normal-2', created_ms: T0 + 2_500, status: 'pending_approval' },
    ]))
    await m.loadTasks()
    expect(m.pendingTasksView.value.map(v => v.id)).toEqual(['orphan', 'normal-2', 'normal-1'])
  })

  it('loadTasks 只保留 pending_approval', async () => {
    const m = await loadModule()
    gwFetchMock.mockResolvedValueOnce(okResp([
      { id: 'a', status: 'pending_approval' },
      { id: 'b', status: 'running' },
    ]))
    await m.loadTasks()
    expect(m.pendingTasks.value.map(t => t.id)).toEqual(['a'])
  })

  it('loadTasks HTTP 非 2xx → 失联态 + 全局错误不静默', async () => {
    const m = await loadModule()
    gwFetchMock.mockResolvedValueOnce({ ok: false, status: 503, json: async () => ({}) })
    await m.loadTasks()
    expect(m.gwDisconnected.value).toBe(true)
    expect(m.pendingTasks.value).toEqual([])
    expect(markGwFailureMock).toHaveBeenCalled()
  })
})

describe('useApproval · decideTask', () => {
  it('拒绝无理由 → 阻止提交，不发请求', async () => {
    const m = await loadModule()
    await m.decideTask('t1', false, false)
    expect(alertMock).toHaveBeenCalledWith('拒绝必须填写理由')
    expect(gwFetchMock).not.toHaveBeenCalled()
  })

  it('拒绝带理由 → POST /approve 携带 note，提交后清理备注并刷新', async () => {
    const m = await loadModule()
    m.rejectNotes.value['t1'] = '危险操作'
    gwFetchMock.mockResolvedValueOnce(okResp([]))
    gwFetchMock.mockResolvedValueOnce(okResp([]))
    await m.decideTask('t1', false, true)
    const [path, opts] = gwFetchMock.mock.calls[0]
    expect(path).toBe('/approve')
    expect(opts.method).toBe('POST')
    expect(JSON.parse(opts.body)).toEqual({ task_id: 't1', approve: false, remember: true, note: '危险操作' })
    expect(m.rejectNotes.value['t1']).toBeUndefined()
    expect(gwFetchMock.mock.calls[1][0]).toBe('/tasks')
  })

  it('提交失败 → 告警不静默', async () => {
    const m = await loadModule()
    gwFetchMock.mockRejectedValueOnce(new Error('boom'))
    await m.decideTask('t1', true, false)
    expect(alertMock).toHaveBeenCalled()
    expect(String(alertMock.mock.calls[0][0])).toContain('审批提交失败')
  })
})

describe('useApproval · cancelTask', () => {
  it('终止成功 → 刷新任务列表，无告警', async () => {
    const m = await loadModule()
    gwFetchMock.mockResolvedValueOnce(okResp([]))
    gwFetchMock.mockResolvedValueOnce(okResp([]))
    await m.cancelTask('t9')
    expect(gwFetchMock.mock.calls[0][0]).toBe('/task/cancel')
    expect(JSON.parse(gwFetchMock.mock.calls[0][1].body)).toEqual({ task_id: 't9' })
    expect(gwFetchMock.mock.calls[1][0]).toBe('/tasks')
    expect(alertMock).not.toHaveBeenCalled()
  })

  it('业务失败 → 弹出后端 error 信息', async () => {
    const m = await loadModule()
    gwFetchMock.mockResolvedValueOnce({
      ok: false,
      status: 409,
      json: async () => ({ error: '任务已在执行，无法终止' }),
    })
    await m.cancelTask('t9')
    expect(alertMock).toHaveBeenCalledWith('终止失败：任务已在执行，无法终止')
  })

  it('请求未完成 → 告警网关不可用', async () => {
    const m = await loadModule()
    gwFetchMock.mockRejectedValueOnce(new Error('econnrefused'))
    await m.cancelTask('t9')
    expect(alertMock).toHaveBeenCalledWith('终止请求未完成：网关不可用')
  })
})
