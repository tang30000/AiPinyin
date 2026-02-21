# -*- coding: utf-8 -*-
"""系统性补充 dict.txt 中缺失的高频词 + 提升已有词权重"""
import os

HIGH_FREQ = [
    ("de","的",999),("shi","是",998),("bu","不",997),("le","了",996),
    ("wo","我",995),("ni","你",994),("ta","他",993),("zhe","这",992),
    ("na","那",991),("you","有",990),("ren","人",989),("zai","在",988),
    ("da","大",987),("shang","上",986),("zhong","中",985),
    ("yi","一",984),("ge","个",983),("lai","来",982),("qu","去",981),
    ("hao","好",980),("ye","也",979),("he","和",978),("shuo","说",977),
    ("dui","对",976),("men","们",975),("dao","到",974),("jiu","就",973),
    ("neng","能",972),("hui","会",971),("dou","都",970),
    ("xiang","想",969),("kan","看",968),("zuo","做",967),
    ("mei","没",966),("xia","下",965),("guo","过",964),
    ("ta","她",960),("ta","它",955),
    ("ziji","自己",950),("women","我们",950),("nimen","你们",940),
    ("tamen","他们",940),("dajia","大家",930),
    ("zhidao","知道",945),("meiyou","没有",945),
    ("yijing","已经",940),("xianzai","现在",940),
    ("yinwei","因为",935),("suoyi","所以",935),
    ("danshi","但是",935),("keshi","可是",930),
    ("haishi","还是",930),("ruguo","如果",925),
    ("huozhe","或者",925),("erqie","而且",920),
    ("suiran","虽然",920),("ranhou","然后",920),
    ("kaishi","开始",920),("jieshu","结束",910),
    ("juede","觉得",915),("xiwang","希望",910),
    ("keneng","可能",920),("yinggai","应该",920),
    ("xuyao","需要",915),("keyi","可以",930),
    ("bushi","不是",945),("jiushi","就是",940),
    ("wenti","问题",940),("shijian","时间",940),
    ("difang","地方",920),("dongxi","东西",915),
    ("xuesheng","学生",910),("laoshi","老师",910),
    ("tongxue","同学",905),("pengyou","朋友",910),
    ("gongzuo","工作",920),("xuexi","学习",915),
    ("shenghuo","生活",915),("shenti","身体",900),
    ("jiankang","健康",895),("kuaile","快乐",890),
    ("zhunbei","准备",880),("jixu","继续",875),
    ("liaojie","了解",870),("mingbai","明白",870),
    ("bangzhu","帮助",860),("tongyi","同意",855),
    ("nuli","努力",860),("renwei","认为",870),
    ("xihuan","喜欢",880),("gaosu","告诉",865),
    ("danxin","担心",860),("fangxin","放心",855),
    ("jueding","决定",860),("jihua","计划",855),
    ("guojia","国家",900),("zhongguo","中国",920),
    ("shehui","社会",890),("jingji","经济",885),
    ("wenhua","文化",880),("jiaoyu","教育",880),
    ("jishu","技术",875),("guanxi","关系",870),
    ("jieguo","结果",860),("fangfa","方法",860),
    ("yisi","意思",855),("xinxi","信息",870),
    ("diannao","电脑",870),("shouji","手机",875),
    ("wangluo","网络",870),("ruanjian","软件",860),
    ("feichang","非常",880),("tebie","特别",870),
    ("yiding","一定",870),("qishi","其实",870),
    ("yizhi","一直",860),("zuijin","最近",860),
    ("nihao","你好",900),("xiexie","谢谢",900),
    ("jintian","今天",870),("zuotian","昨天",860),("mingtian","明天",860),
    ("gaoxing","高兴",870),("kaixin","开心",870),
    ("pibei","疲惫",850),
    ("chengxu","程序",860),("wenjian","文件",860),
    ("gongneng","功能",855),("shezhi","设置",855),
    ("yige","一个",900),("zhege","这个",900),
    ("nage","那个",895),("yixie","一些",870),
    ("shiqing","事情",880),("shihou","时候",890),
    ("weile","为了",880),("zhongyao","重要",880),
    ("jiandan","简单",860),("fangbian","方便",860),
    ("anquan","安全",860),
]

def patch_dict(path):
    hf = {(py, word): w for py, word, w in HIGH_FREQ}
    existing = set()
    with open(path, 'r', encoding='utf-8') as f:
        for line in f:
            parts = line.strip().split(',', 2)
            if len(parts) >= 2:
                existing.add((parts[0].strip(), parts[1].strip()))

    boosted = added = 0
    out = []
    with open(path, 'r', encoding='utf-8') as f:
        for line in f:
            parts = line.strip().split(',', 2)
            if len(parts) >= 3:
                py, word = parts[0].strip(), parts[1].strip()
                try:
                    old_w = int(parts[2].strip())
                    if (py, word) in hf and hf[(py, word)] > old_w:
                        line = f"{py},{word},{hf[(py, word)]}\n"
                        boosted += 1
                except ValueError:
                    pass
            out.append(line if line.endswith('\n') else line + '\n')

    for py, word, w in HIGH_FREQ:
        if (py, word) not in existing:
            out.append(f"{py},{word},{w}\n")
            added += 1

    with open(path, 'w', encoding='utf-8') as f:
        f.writelines(out)
    print(f"  +{added} 新词, ↑{boosted} 提权")

if __name__ == '__main__':
    base = os.path.dirname(os.path.dirname(__file__))
    for p in ['target/debug/dict.txt', 'dict.txt']:
        full = os.path.join(base, p)
        if os.path.exists(full):
            print(f"处理: {full}")
            patch_dict(full)
