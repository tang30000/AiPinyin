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
    # === 长句输入需要的高频词 (字典中存在但权重=100) ===
    ("sudu","速度",900),("tianqi","天气",910),("shijie","世界",910),
    ("guji","估计",920),("bijiao","比较",900),("zhengchang","正常",880),
    ("dasuan","打算",870),("diqiu","地球",860),("dianhua","电话",900),
    ("duoshao","多少",890),("faxian","发现",890),("ganqing","感情",870),
    ("ganjue","感觉",900),("gaoxing","高兴",880),("guanyu","关于",880),
    ("huiyi","回忆",860),("huanjing","环境",870),("jiating","家庭",880),
    ("jiandan","简单",870),("jianchi","坚持",860),("jiedao","街道",850),
    ("jiemu","节目",855),("jingli","经历",860),("jingyan","经验",870),
    ("juese","角色",860),("likai","离开",870),("lihai","厉害",860),
    ("lixiang","理想",860),("maishang","马上",880),("nianqing","年轻",860),
    ("piaoliang","漂亮",870),("pingguo","苹果",865),("qingkuang","情况",880),
    ("queding","确定",870),("renwu","任务",860),("shengri","生日",870),
    ("shuiping","水平",860),("taidu","态度",860),("tiaojian","条件",870),
    ("tongshi","同时",870),("tongyi","同意",860),("wanquan","完全",870),
    ("weixian","危险",860),("xiguan","习惯",870),("xinqing","心情",870),
    ("yaoqiu","要求",870),("yihou","以后",890),("yiqian","以前",890),
    ("yijian","意见",870),("yongyuan","永远",860),("youxiu","优秀",860),
    ("yuanyin","原因",870),("yuanwang","愿望",855),("zenmyang","怎样",870),
    ("zenme","怎么",900),("zhengzai","正在",880),("zhichi","支持",870),
    ("zhuyi","注意",880),("zhuanye","专业",860),("ziran","自然",870),
    ("zuoye","作业",860),("zuoyou","左右",860),("zuowei","作为",870),
    ("mashang","马上",890),("huoxu","或许",880),("zuihou","最后",880),
    ("bieren","别人",870),("bixu","必须",880),("chengshi","城市",880),
    ("daxue","大学",890),("diying","电影",880),("duanqi","短期",850),
    ("fazhan","发展",880),("fuwu","服务",860),("ganxie","感谢",860),
    ("gongsi","公司",890),("guanli","管理",870),("huodong","活动",870),
    ("jiaotong","交通",870),("jizhong","集中",855),("liyou","理由",860),
    ("meili","美丽",860),("muqian","目前",870),("nengli","能力",870),
    ("peiyang","培养",855),("qiantu","前途",850),("quanbu","全部",870),
    ("renshi","认识",880),("shenme","什么",940),("shengyi","生意",860),
    ("shuohua","说话",870),("teshu","特殊",855),("tuijian","推荐",860),
    ("xingqu","兴趣",860),("xinwen","新闻",870),("xuanze","选择",870),
    ("yanjiu","研究",870),("yiliao","医疗",860),("youhao","友好",855),
    ("yuyan","语言",870),("zhengfu","政府",870),("zhishi","知识",870),
    ("zhuti","主题",860),("ziyou","自由",870),
    # === 日常打字常用词 (第二批) ===
    ("genghao","更好",920),("gengduo","更多",920),("chulai","出来",910),
    ("huilai","回来",910),("qilai","起来",910),("xiaqu","下去",890),
    ("chengwei","成为",880),("tongshi","同事",870),("tongxue","同学",870),
    ("weilai","未来",890),("renke","认可",870),("tonguo","通过",900),
    ("tongguo","通过",900),("xuanze","选择",880),("xuexi","学习",920),
    ("gaige","改革",860),("shishi","实施",860),("fenxi","分析",870),
    ("yingxiang","影响",880),("jiegou","结构",860),("zhiliang","质量",870),
    ("zhidu","制度",860),("guimo","规模",855),("jiance","检测",855),
    ("sheji","设计",870),("jianshe","建设",860),("biaozhun","标准",870),
    ("ziwei","滋味",850),("wanzheng","完整",870),("jingque","精确",860),
    ("zhengque","正确",880),("cuowu","错误",870),("daibiao","代表",870),
    ("biaoxian","表现",870),("biaoshi","表示",870),("chenggong","成功",880),
    ("shibai","失败",870),("zhunque","准确",870),("youshi","优势",860),
    ("lieshi","劣势",850),("xianshi","现实",870),("lilun","理论",860),
    ("shijian","实践",860),("jingji","经济",880),("zhengce","政策",870),
    ("zhichi","支持",870),("fandui","反对",870),("tongyi","同意",860),
    ("jiben","基本",870),("zhuyao","主要",880),("zhongyao","重要",890),
    ("guanjian","关键",870),("yiban","一般",870),("tebie","特别",880),
    ("qita","其他",870),("suoyou","所有",870),("meige","每个",860),
    ("zheyang","这样",890),("nayang","那样",880),("yiyang","一样",890),
    ("butong","不同",880),("xiangtong","相同",870),("bici","彼此",850),
    ("huran","忽然",860),("turan","突然",870),("yiran","依然",860),
    ("zhongjiu","终究",850),("zhongyu","终于",880),("conglai","从来",870),
    ("yijing","已经",940),("zhengzai","正在",890),("jijiang","即将",870),
    ("daodi","到底",870),("jiran","既然",870),("fanzheng","反正",860),
    ("dagai","大概",870),("bingqie","并且",860),("lingwai","另外",870),
    ("shunbian","顺便",855),("suibian","随便",860),("danran","当然",890),
    ("dangran","当然",890),("haoxiang","好像",880),("sihu","似乎",860),
    ("zhende","真的",890),("queshi","确实",880),("shifen","十分",860),
    ("zhidao","知道",940),("buzhidao","不知道",930),("mingbai","明白",870),
    ("liaojie","了解",870),("juede","觉得",910),("renwei","认为",880),
    ("xiwang","希望",910),("xianxin","相信",880),("xiangxin","相信",880),
    ("danxin","担心",870),("guanxin","关心",870),("kaixin","开心",880),
    ("shangxin","伤心",860),("gaoxing","高兴",880),("nanguo","难过",860),
    ("shenghuo","生活",910),("gongzuo","工作",920),("jiankang","健康",880),
    ("anpai","安排",870),("chufa","出发",870),("daoda","到达",860),
    ("chifan","吃饭",880),("shuijiao","睡觉",870),("xiuxi","休息",870),
    ("yundong","运动",870),("lvxing","旅行",870),("lvyou","旅游",870),
    ("gouwu","购物",860),("zhaogu","照顾",860),("baohu","保护",870),
    ("bangmang","帮忙",860),("xianzai","现在",940),
    ("shijian","时间",940),("didian","地点",860),("neirong","内容",870),
    ("fangshi","方式",870),("fangfa","方法",870),("mubiao","目标",870),
    ("duiyu","对于",880),("guanyu","关于",880),
    ("diannao","电脑",880),("shouji","手机",890),
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
