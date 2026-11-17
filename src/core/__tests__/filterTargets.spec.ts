import { describe, it, expect } from 'vitest'
import { filterTargets, type FilterableTarget } from '../../shared/filterTargets'

const sample: FilterableTarget[] = [
  { name: 'prod-web', host: '10.0.0.224', user: 'root' },
  { name: 'db-node', host: '10.0.0.225', user: 'deploy' },
  { name: 'jump', host: 'bastion.example.com', user: 'ops' },
]

describe('filterTargets', () => {
  it('空查询或纯空白时原样返回全部', () => {
    expect(filterTargets(sample, '')).toBe(sample)
    expect(filterTargets(sample, '   ')).toBe(sample)
  })

  it('按名称子串匹配', () => {
    expect(filterTargets(sample, 'web')).toHaveLength(1)
    expect(filterTargets(sample, 'db-node')[0].host).toBe('10.0.0.225')
  })

  it('按主机地址匹配', () => {
    expect(filterTargets(sample, '10.0.0.22')).toHaveLength(2)
    expect(filterTargets(sample, 'bastion')).toHaveLength(1)
  })

  it('按用户名匹配', () => {
    expect(filterTargets(sample, 'deploy')).toHaveLength(1)
    expect(filterTargets(sample, 'ops')).toHaveLength(1)
  })

  it('不区分大小写', () => {
    expect(filterTargets(sample, 'PROD-WEB')).toHaveLength(1)
    expect(filterTargets(sample, 'BASTION.EXAMPLE.COM')).toHaveLength(1)
  })

  it('无匹配返回空数组', () => {
    expect(filterTargets(sample, 'nonexistent')).toEqual([])
  })

  it('不修改入参数组', () => {
    const before = sample.length
    filterTargets(sample, 'web')
    expect(sample).toHaveLength(before)
  })
})
