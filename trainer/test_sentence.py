# -*- coding: utf-8 -*-
"""测试长句自回归生成"""
import os, json, numpy as np, onnxruntime as ort
os.environ['CUDA_VISIBLE_DEVICES'] = ''
OUT = os.path.join(os.path.dirname(os.path.dirname(__file__)), 'target', 'debug')
s = ort.InferenceSession(os.path.join(OUT, 'weights.onnx'))
py2id = json.load(open(os.path.join(OUT, 'pinyin2id.json'), 'r', encoding='utf-8'))
ch2id = json.load(open(os.path.join(OUT, 'char2id.json'), 'r', encoding='utf-8'))
id2ch = {v: k for k, v in ch2id.items()}
p2c = json.load(open(os.path.join(OUT, 'pinyin2char.json'), 'r', encoding='utf-8'))
CLS = ch2id['<sos>']

def run(ids):
    inp = np.array([ids], dtype=np.int64)
    return s.run(None, {
        'input_ids': inp,
        'attention_mask': np.ones_like(inp),
        'position_ids': np.arange(len(ids), dtype=np.int64).reshape(1, -1)
    })[0]

def split_pinyin(s):
    """简单拼音切分"""
    initials = ['zh','ch','sh','b','p','m','f','d','t','n','l','g','k','h',
                'j','q','x','r','z','c','s','y','w']
    finals = ['iang','iong','uang','ang','eng','ing','ong','uai','uan',
              'ian','iao','uan','uei','uen','ai','ei','ao','ou','an','en',
              'in','un','ia','ie','iu','uo','ua','ue','ui','er',
              'a','o','e','i','u','v']
    result = []
    i = 0
    while i < len(s):
        # try to match an initial
        init = ''
        for ini in initials:
            if s[i:].startswith(ini):
                init = ini
                break
        if init:
            i += len(init)
            # try to match a final
            fin = ''
            for f in finals:
                if s[i:].startswith(f):
                    fin = f
                    break
            if fin:
                result.append(init + fin)
                i += len(fin)
            else:
                result.append(init)
        else:
            # standalone final
            fin = ''
            for f in finals:
                if s[i:].startswith(f):
                    fin = f
                    break
            if fin:
                result.append(fin)
                i += len(fin)
            else:
                i += 1  # skip unknown
    return result

def generate_sentence(pinyin_str, context=''):
    """自回归生成完整句子"""
    syllables = split_pinyin(pinyin_str)
    print(f"拼音: {pinyin_str}")
    print(f"音节: {' '.join(syllables)}")
    
    # 构建 Concat 上下文
    c2py = {}
    for py, chars in p2c.items():
        for c in chars:
            if c not in c2py:
                c2py[c] = py
    
    ids = [CLS]
    # 加入已有上下文
    for c in context:
        py = c2py.get(c)
        if py and py in py2id and c in ch2id:
            ids.append(py2id[py])
            ids.append(ch2id[c])
    
    VOCAB_SIZE = 21571
    result = ''
    
    for syl in syllables:
        if syl not in py2id:
            print(f"  跳过未知音节: {syl}")
            continue
        ids.append(py2id[syl])
        
        logits = run(ids)
        last_logits = logits[0, -1, :]
        
        # 拼音约束: 只在该拼音对应的候选字中选
        cands = p2c.get(syl, [])
        scores = [(c, float(last_logits[ch2id[c]])) for c in cands if c in ch2id]
        scores.sort(key=lambda x: -x[1])
        
        if scores:
            best_char = scores[0][0]
            result += best_char
            ids.append(ch2id[best_char])
            top3 = ', '.join(f'{c}={s:.1f}' for c, s in scores[:3])
            print(f"  [{syl:4s}] → {best_char}  (top3: {top3})")
        else:
            print(f"  [{syl:4s}] → ?  (无候选)")
    
    print(f"\n结果: {result}")
    return result

print("=" * 60)
print("测试1: 我估计明天会下雪")
generate_sentence("wogujimingtianhuixiaxue")

print("\n" + "=" * 60)
print("测试2: 今天天气不错 (有上下文)")
generate_sentence("jintiantiaqibucuo")

print("\n" + "=" * 60)
print("测试3: 你好世界")
generate_sentence("nihaoshijie")

print("\n" + "=" * 60) 
print("测试4: 工作了一天感觉很累")
generate_sentence("gongzuoleyitianganjiuehenlei")

print("\n" + "=" * 60)
print("测试5: 如果明天还下雪我就不去了 (超长)")
generate_sentence("ruguomingtianhaioxiaxuewojuibuqule")
