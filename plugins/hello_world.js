/**
 * AiPinyin 示例插件 — hello_world.js
 *
 * 插件 API:
 *   function on_candidates(raw, candidates)
 *     @param raw        当前输入的拼音字母串，如 "shi"
 *     @param candidates 引擎给出的候选词数组，如 ["是","时","十",...]
 *     @return           修改后的候选词数组（可以完全替换或追加）
 *
 * 调试：console.log() 输出到控制台
 */

// ── 内置快捷词 ──────────────────────────────────────────────

/** 输入 'time' 返回当前时间 */
var SHORTCUTS = {
    'time': function () {
        var d = new Date();
        return [pad2(d.getHours()) + ':' + pad2(d.getMinutes()) + ':' + pad2(d.getSeconds())];
    },
    'date': function () {
        var d = new Date();
        return [d.getFullYear() + '-' + pad2(d.getMonth() + 1) + '-' + pad2(d.getDate())];
    },
    'week': function () {
        var days = ['日', '一', '二', '三', '四', '五', '六'];
        return ['星期' + days[new Date().getDay()]];
    }
};

/**
 * 主钩子 — 每次候选词更新时被调用
 * 返回 candidates 不变则走普通拼音流程
 */
function on_candidates(raw, candidates) {
    if (SHORTCUTS[raw]) {
        var extra = SHORTCUTS[raw]();
        console.log('快捷词 [' + raw + '] → ' + extra[0]);
        // 插入到最前，保留原始候选
        return extra.concat(Array.prototype.slice.call(candidates));
    }
    return candidates;
}

// ── 工具函数 ────────────────────────────────────────────────

function pad2(n) {
    return n < 10 ? '0' + n : '' + n;
}
