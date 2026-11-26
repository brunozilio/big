import type { StageView } from '@/api/contract'

export function stageStep(stage: StageView) {
  return { id: stage.id, label: stage.label, icon: stage.icon, state: stage.state, attempts: 0, seconds: 0 }
}
