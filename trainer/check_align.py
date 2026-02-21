import json

py = json.load(open('target/debug/pinyin2id.json', 'r', encoding='utf-8'))

# 检查常见音节
test = ['zhi','chi','shi','ri','zi','ci','si','yu','yue','yuan','yun',
        'nv','lv','xian','xiang','zhuang','chuang','shuang','er','ang','ei','en','ou',
        'a','o','e','ai','ao','an']

for t in test:
    print(f"  {t}: {'OK' if t in py else 'MISS'}")

# 检查我们 split_pinyin 的音节表是否都在模型词表里
# 我们的完整音节表 (从 pinyin.rs INITIALS + FINALS 组合)
our_syllables = [
    'ba','pa','ma','fa','da','ta','na','la','ga','ka','ha','za','ca','sa',
    'zha','cha','sha','ra','jia','qia','xia',
    'bo','po','mo','fo','lo','duo','tuo','nuo','luo','guo','kuo','huo','zuo','cuo','suo',
    'zhuo','chuo','shuo','ruo',
    'bi','pi','mi','di','ti','ni','li','ji','qi','xi',
    'bu','pu','mu','fu','du','tu','nu','lu','gu','ku','hu','zu','cu','su',
    'zhu','chu','shu','ru',
    'bai','pai','mai','dai','tai','nai','lai','gai','kai','hai','zai','cai','sai',
    'zhai','chai','shai',
    'bei','pei','mei','fei','dei','nei','lei','gei','kei','hei','zei','shei',
    'ban','pan','man','fan','dan','tan','nan','lan','gan','kan','han','zan','can','san',
    'zhan','chan','shan','ran',
    'ben','pen','men','fen','den','nen','gen','ken','hen','zen','cen','sen',
    'zhen','chen','shen','ren',
    'shi','zhi','chi','ri','zi','ci','si',
    'yu','yue','yuan','yun',
    'nv','lv','xian','xiang','zhuang',
    'wo','ni','hao','shi','de',
]

missing = []
for syl in our_syllables:
    if syl not in py:
        missing.append(syl)

if missing:
    print(f"\n我们的音节中模型缺失: {missing}")
else:
    print(f"\n所有常用音节都在模型词表中!")
