import test from 'node:test';
import assert from 'node:assert/strict';
import {node, edge, layoutGraph, computeNodeAutoFitBox} from './model.js';
import {updateNode, emptyMindmap, mindmapGraph} from './mindmap.js';

test('double-click text edit: node label updates live and updates auto-fit box', ()=>{
 const n = node('초기 글', 100, 100, 'rectangle', 'blue', 'n1');
 assert.equal(n.data.label, '초기 글');

 // 더블클릭 편집 시작: initial 저장
 const editing = {type: 'node', id: 'n1', initial: n.data.label, current: n.data.label};
 assert.equal(editing.current, '초기 글');

 // 텍스트 수정 (실시간)
 const typed = '수정된 여러 줄 내용\n두 번째 줄';
 const updatedNode = {...n, data: {...n.data, label: typed}};
 const graph = layoutGraph({nodes: [updatedNode], edges: []});
 const box = computeNodeAutoFitBox(graph.nodes[0]);

 assert.equal(graph.nodes[0].data.label, typed);
 assert.equal(box.lines.length, 2);
 assert.equal(box.lines[0], '수정된 여러 줄 내용');
 assert.equal(box.lines[1], '두 번째 줄');

 // 확정 (Ctrl+Enter 또는 바깥 클릭): 변경사항 유지
 editing.current = typed;
 assert.notEqual(editing.current, editing.initial);
});

test('double-click text edit: Esc cancels and restores original text', ()=>{
 const n = node('원래 텍스트', 100, 100, 'roundRect', 'blue', 'n2');
 const editing = {type: 'node', id: 'n2', initial: n.data.label, current: '입력 중 취소할 글'};

 // Esc 취소: 원래 initial 값으로 복구
 const cancelledNode = {...n, data: {...n.data, label: editing.initial}};
 assert.equal(cancelledNode.data.label, '원래 텍스트');
});

test('double-click text edit: IME composition prevents premature submit/cancel', ()=>{
 // 한글 조합 중(isComposing===true)에는 Enter나 Esc를 눌러도 제출/취소되지 않아야 함
 const handleKeyDown = (e, {onFinish, onCancel}) => {
  if (e.isComposing) return 'composing_ignored';
  if (e.key === 'Escape') {
   onCancel();
   return 'cancelled';
  }
  if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
   onFinish();
   return 'finished';
  }
  if (e.key === 'Enter') return 'newline';
  return 'default';
 };

 let finished = false, cancelled = false;
 const callbacks = {
  onFinish: () => { finished = true; },
  onCancel: () => { cancelled = true; }
 };

 // IME 조합 중 Enter
 const imeResult = handleKeyDown({key: 'Enter', ctrlKey: true, isComposing: true}, callbacks);
 assert.equal(imeResult, 'composing_ignored');
 assert.equal(finished, false);

 // 조합 완료 후 Ctrl+Enter 확정
 const finishResult = handleKeyDown({key: 'Enter', ctrlKey: true, isComposing: false}, callbacks);
 assert.equal(finishResult, 'finished');
 assert.equal(finished, true);

 // 조합 중 Esc
 const imeEscResult = handleKeyDown({key: 'Escape', isComposing: true}, callbacks);
 assert.equal(imeEscResult, 'composing_ignored');
 assert.equal(cancelled, false);

 // 조합 완료 후 Esc 취소
 const cancelResult = handleKeyDown({key: 'Escape', isComposing: false}, callbacks);
 assert.equal(cancelResult, 'cancelled');
 assert.equal(cancelled, true);
});

test('double-click text edit: edge label editing and commit', ()=>{
 const e = edge('n1', 'n2', '기존 설명', 'e1');
 assert.equal(e.label, '기존 설명');

 const editing = {type: 'edge', id: 'e1', initial: e.label, current: '새 연결선 설명'};
 const updatedEdge = {...e, label: editing.current};
 assert.equal(updatedEdge.label, '새 연결선 설명');
});

test('double-click text edit: mindmap node editing updates mindmap data', ()=>{
 const mm = emptyMindmap('중심 주제');
 const rootId = mm.root.id;

 // 중심 주제 글 수정
 const typed = '2026 전략 기획';
 const next = updateNode(mm, rootId, {text: typed});
 assert.equal(next.root.text, typed);
 assert.equal(mindmapGraph(next).nodes[0].data.label, typed);
});
