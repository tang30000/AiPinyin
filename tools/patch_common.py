# -*- coding: utf-8 -*-
"""补充字典中缺失的高频词组"""

# 格式: 拼音,词,权重
MISSING_WORDS = """
haishi,还是,900
huanshi,还是,500
suoyi,所以,900
yinwei,因为,900
danshi,但是,900
keshi,可是,880
buguo,不过,870
ranhou,然后,860
yijing,已经,850
xianzai,现在,850
haiyou,还有,840
haishi,海水,200
haihao,还好,800
yihou,以后,830
yiqian,以前,830
zuijin,最近,820
bieru,比如,810
huozhe,或者,900
erqie,而且,890
suiran,虽然,870
zhiyao,只要,860
zhiyou,只有,850
buru,不如,800
chule,除了,800
jiushi,就是,900
zongshi,总是,850
yinggai,应该,880
keneng,可能,870
xiwang,希望,860
juede,觉得,850
tongyi,同意,840
xuyao,需要,870
yiyang,一样,830
meiyou,没有,900
bushi,不是,950
deshi,的时,200
haoxiang,好像,820
zaijian,再见,800
xièxiè,谢谢,500
xiexie,谢谢,900
duibuqi,对不起,800
meiguanxi,没关系,790
zenmeyang,怎么样,780
shenmeyang,什么样,770
bieren,别人,800
renmen,人们,790
womende,我们的,750
tamende,他们的,750
yizhong,一种,700
yige,一个,900
zhege,这个,900
nage,那个,890
naxie,那些,800
zhexie,这些,800
dagai,大概,780
qishi,其实,870
shishi,事实,750
shishishang,事实上,740
zhidao,知道,900
mingbai,明白,850
liaojie,了解,840
kaishi,开始,870
jieshu,结束,850
jixu,继续,840
gongzuo,工作,900
xuexi,学习,870
shenghuo,生活,870
wenti,问题,950
yisi,意思,830
yijian,意见,800
fangfa,方法,830
jieguo,结果,820
yuanyin,原因,810
qingkuang,情况,800
huanjing,环境,790
guanxi,关系,830
yingxiang,影响,800
diannao,电脑,850
shouji,手机,850
wangluo,网络,840
ruanjian,软件,830
xitong,系统,830
shuju,数据,820
pibei,疲惫,800
gaoxing,高兴,850
kaixin,开心,850
nanguo,难过,800
shengqi,生气,800
haipa,害怕,790
danxin,担心,800
fangxin,放心,790
""".strip()

import os

def patch_dict(dict_path):
    existing = set()
    with open(dict_path, 'r', encoding='utf-8') as f:
        for line in f:
            parts = line.strip().split(',', 2)
            if len(parts) >= 2:
                existing.add((parts[0].strip(), parts[1].strip()))

    added = 0
    with open(dict_path, 'a', encoding='utf-8') as f:
        for line in MISSING_WORDS.split('\n'):
            line = line.strip()
            if not line: continue
            parts = line.split(',', 2)
            if len(parts) < 2: continue
            key = (parts[0].strip(), parts[1].strip())
            if key not in existing:
                f.write(line + '\n')
                added += 1
    print(f"  补充 {added} 个缺失词条")

if __name__ == '__main__':
    for p in ['target/debug/dict.txt', 'dict.txt']:
        full = os.path.join(os.path.dirname(os.path.dirname(__file__)), p)
        if os.path.exists(full):
            print(f"处理: {full}")
            patch_dict(full)
