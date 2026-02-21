"""
AiPinyin 训练语料生成器

生成 pinyin\t汉字 格式的训练数据。
每行一个句子，拼音和汉字一一对应。

用法:
  python trainer/gen_corpus.py
  → 输出到 trainer/data/corpus.txt
"""

import os
import random

# ============================================================
# 核心词库: (拼音列表, 汉字字符串)
# 每个音节对应一个汉字
# ============================================================

# --- 高频日常用语 ---
DAILY = [
    # 问候
    ("ni hao", "你好"),
    ("da jia hao", "大家好"),
    ("zao shang hao", "早上好"),
    ("wan shang hao", "晚上好"),
    ("xia wu hao", "下午好"),
    ("ni hao ma", "你好吗"),
    ("wo hen hao", "我很好"),
    ("xie xie", "谢谢"),
    ("xie xie ni", "谢谢你"),
    ("bu ke qi", "不客气"),
    ("dui bu qi", "对不起"),
    ("mei guan xi", "没关系"),
    ("zai jian", "再见"),
    ("ming tian jian", "明天见"),
    ("wan an", "晚安"),
    ("qing wen", "请问"),

    # 自我介绍
    ("wo shi", "我是"),
    ("wo jiao", "我叫"),
    ("wo de ming zi", "我的名字"),
    ("wo shi zhong guo ren", "我是中国人"),
    ("wo shi xue sheng", "我是学生"),
    ("wo shi lao shi", "我是老师"),
    ("wo zai gong zuo", "我在工作"),
    ("wo xi huan", "我喜欢"),
    ("wo ai ni", "我爱你"),

    # 时间
    ("jin tian", "今天"),
    ("ming tian", "明天"),
    ("zuo tian", "昨天"),
    ("xian zai", "现在"),
    ("jin nian", "今年"),
    ("qu nian", "去年"),
    ("ming nian", "明年"),
    ("shang wu", "上午"),
    ("xia wu", "下午"),
    ("wan shang", "晚上"),
    ("zao shang", "早上"),
    ("zhong wu", "中午"),
    ("mei tian", "每天"),
    ("yi ge yue", "一个月"),
    ("yi nian", "一年"),
    ("xing qi yi", "星期一"),
    ("xing qi er", "星期二"),
    ("xing qi san", "星期三"),
    ("xing qi si", "星期四"),
    ("xing qi wu", "星期五"),
    ("xing qi liu", "星期六"),
    ("xing qi ri", "星期日"),

    # 数字和量词
    ("yi ge", "一个"),
    ("liang ge", "两个"),
    ("san ge", "三个"),
    ("si ge", "四个"),
    ("wu ge", "五个"),
    ("liu ge", "六个"),
    ("qi ge", "七个"),
    ("ba ge", "八个"),
    ("jiu ge", "九个"),
    ("shi ge", "十个"),
    ("duo shao", "多少"),
    ("duo shao qian", "多少钱"),
    ("ji ge", "几个"),

    # 方位
    ("shang mian", "上面"),
    ("xia mian", "下面"),
    ("li mian", "里面"),
    ("wai mian", "外面"),
    ("qian mian", "前面"),
    ("hou mian", "后面"),
    ("zuo bian", "左边"),
    ("you bian", "右边"),
    ("zhong jian", "中间"),
    ("pang bian", "旁边"),
    ("fu jin", "附近"),

    # 饮食
    ("chi fan", "吃饭"),
    ("he shui", "喝水"),
    ("he cha", "喝茶"),
    ("he ka fei", "喝咖啡"),
    ("chi zao fan", "吃早饭"),
    ("chi wu fan", "吃午饭"),
    ("chi wan fan", "吃晚饭"),
    ("wo e le", "我饿了"),
    ("wo bao le", "我饱了"),
    ("hao chi", "好吃"),
    ("tai hao chi le", "太好吃了"),
    ("mian bao", "面包"),
    ("mi fan", "米饭"),
    ("shui guo", "水果"),
    ("ping guo", "苹果"),
    ("xi gua", "西瓜"),
    ("niu nai", "牛奶"),
    ("ji dan", "鸡蛋"),
    ("niu rou", "牛肉"),
    ("zhu rou", "猪肉"),
    ("ji rou", "鸡肉"),
    ("yu", "鱼"),
    ("qing cai", "青菜"),
    ("fan qie", "番茄"),
    ("tu dou", "土豆"),

    # 天气
    ("jin tian tian qi hen hao", "今天天气很好"),
    ("jin tian xia yu le", "今天下雨了"),
    ("ming tian hui xia yu ma", "明天会下雨吗"),
    ("tian qi hen re", "天气很热"),
    ("tian qi hen leng", "天气很冷"),
    ("xia xue le", "下雪了"),
    ("gua feng le", "刮风了"),
    ("yin tian", "阴天"),
    ("qing tian", "晴天"),
    ("duo yun", "多云"),
    ("wen du", "温度"),

    # 交通出行
    ("zuo gong jiao che", "坐公交车"),
    ("da di", "打的"),
    ("kai che", "开车"),
    ("qi zi xing che", "骑自行车"),
    ("zuo di tie", "坐地铁"),
    ("zuo huo che", "坐火车"),
    ("zuo fei ji", "坐飞机"),
    ("ji chang", "机场"),
    ("huo che zhan", "火车站"),
    ("qi che zhan", "汽车站"),
    ("gong jiao zhan", "公交站"),
    ("gao su gong lu", "高速公路"),
    ("hong lv deng", "红绿灯"),

    # 购物
    ("mai dong xi", "买东西"),
    ("duo shao qian", "多少钱"),
    ("tai gui le", "太贵了"),
    ("pian yi yi dian", "便宜一点"),
    ("ke yi shua ka ma", "可以刷卡吗"),
    ("wo yao mai", "我要买"),
    ("chao shi", "超市"),
    ("shang dian", "商店"),

    # 工作学习
    ("shang ban", "上班"),
    ("xia ban", "下班"),
    ("gong zuo", "工作"),
    ("xue xi", "学习"),
    ("kai hui", "开会"),
    ("xie bao gao", "写报告"),
    ("fa you jian", "发邮件"),
    ("lao ban", "老板"),
    ("tong shi", "同事"),
    ("xue xiao", "学校"),
    ("da xue", "大学"),
    ("zhong xue", "中学"),
    ("xiao xue", "小学"),
    ("lao shi", "老师"),
    ("xue sheng", "学生"),
    ("ke ben", "课本"),
    ("zuo ye", "作业"),
    ("kao shi", "考试"),
    ("cheng ji", "成绩"),
    ("bi ye", "毕业"),

    # 科技
    ("dian nao", "电脑"),
    ("shou ji", "手机"),
    ("wang luo", "网络"),
    ("ruan jian", "软件"),
    ("ying jian", "硬件"),
    ("hu lian wang", "互联网"),
    ("ren gong zhi neng", "人工智能"),
    ("shu ju", "数据"),
    ("cheng xu", "程序"),
    ("cheng xu yuan", "程序员"),
    ("kai fa", "开发"),
    ("ce shi", "测试"),
    ("she ji", "设计"),
    ("xi tong", "系统"),
    ("fu wu qi", "服务器"),
    ("shu ju ku", "数据库"),
    ("yun ji suan", "云计算"),
    ("da shu ju", "大数据"),
    ("shen du xue xi", "深度学习"),
    ("ji qi xue xi", "机器学习"),
    ("zi ran yu yan chu li", "自然语言处理"),

    # 情感
    ("gao xing", "高兴"),
    ("kai xin", "开心"),
    ("nan guo", "难过"),
    ("sheng qi", "生气"),
    ("hai pa", "害怕"),
    ("dan xin", "担心"),
    ("ji dong", "激动"),
    ("wu liao", "无聊"),
    ("lei le", "累了"),
    ("xin ku le", "辛苦了"),
    ("jia you", "加油"),
    ("hao de", "好的"),
    ("mei wen ti", "没问题"),
    ("dang ran", "当然"),
    ("ke neng", "可能"),
    ("yi ding", "一定"),
    ("fei chang hao", "非常好"),
    ("fei chang gan xie", "非常感谢"),

    # 家庭
    ("ba ba", "爸爸"),
    ("ma ma", "妈妈"),
    ("ge ge", "哥哥"),
    ("jie jie", "姐姐"),
    ("di di", "弟弟"),
    ("mei mei", "妹妹"),
    ("ye ye", "爷爷"),
    ("nai nai", "奶奶"),
    ("jia ren", "家人"),
    ("peng you", "朋友"),
    ("nan peng you", "男朋友"),
    ("nv peng you", "女朋友"),
    ("hai zi", "孩子"),
    ("er zi", "儿子"),
    ("nv er", "女儿"),
    ("lao gong", "老公"),
    ("lao po", "老婆"),

    # 身体 / 健康
    ("shen ti", "身体"),
    ("jian kang", "健康"),
    ("yi yuan", "医院"),
    ("yi sheng", "医生"),
    ("yao", "药"),
    ("chi yao", "吃药"),
    ("tou teng", "头疼"),
    ("du zi teng", "肚子疼"),
    ("fa shao", "发烧"),
    ("gan mao le", "感冒了"),
    ("duan lian shen ti", "锻炼身体"),
    ("pao bu", "跑步"),
    ("you yong", "游泳"),
    ("da qiu", "打球"),
    ("ti zu qiu", "踢足球"),
    ("da lan qiu", "打篮球"),

    # 地理
    ("zhong guo", "中国"),
    ("bei jing", "北京"),
    ("shang hai", "上海"),
    ("guang zhou", "广州"),
    ("shen zhen", "深圳"),
    ("cheng du", "成都"),
    ("hang zhou", "杭州"),
    ("nan jing", "南京"),
    ("xi an", "西安"),
    ("wu han", "武汉"),
    ("chang sha", "长沙"),
    ("tian jin", "天津"),
    ("chong qing", "重庆"),
    ("da lian", "大连"),
    ("qing dao", "青岛"),
    ("su zhou", "苏州"),
    ("dong jing", "东京"),
    ("niu yue", "纽约"),
    ("lun dun", "伦敦"),
    ("ba li", "巴黎"),

    # 自然
    ("tai yang", "太阳"),
    ("yue liang", "月亮"),
    ("xing xing", "星星"),
    ("da shan", "大山"),
    ("da hai", "大海"),
    ("he liu", "河流"),
    ("hu po", "湖泊"),
    ("shu mu", "树木"),
    ("hua duo", "花朵"),
    ("cao di", "草地"),
    ("sen lin", "森林"),
    ("dong wu", "动物"),
    ("xiao niao", "小鸟"),
    ("xiao gou", "小狗"),
    ("xiao mao", "小猫"),

    # 颜色
    ("hong se", "红色"),
    ("lan se", "蓝色"),
    ("lv se", "绿色"),
    ("huang se", "黄色"),
    ("bai se", "白色"),
    ("hei se", "黑色"),
    ("fen se", "粉色"),
    ("zi se", "紫色"),
    ("cheng se", "橙色"),
    ("hui se", "灰色"),
]

# --- 完整句子 ---
SENTENCES = [
    ("wo jin tian hen gao xing", "我今天很高兴"),
    ("ta shi wo de peng you", "他是我的朋友"),
    ("wo men yi qi chi fan ba", "我们一起吃饭吧"),
    ("ni zhi dao ma", "你知道吗"),
    ("wo bu zhi dao", "我不知道"),
    ("qing ni bang wo yi xia", "请你帮我一下"),
    ("ni xiang chi shen me", "你想吃什么"),
    ("wo xi huan chi zhong guo cai", "我喜欢吃中国菜"),
    ("jin tian shi xing qi ji", "今天是星期几"),
    ("jin tian shi xing qi wu", "今天是星期五"),
    ("xian zai ji dian le", "现在几点了"),
    ("wo liu dian qi chuang", "我六点起床"),
    ("ta mei tian dou hen nu li", "他每天都很努力"),
    ("zhe ge hen hao chi", "这个很好吃"),
    ("na ge tai gui le", "那个太贵了"),
    ("ni qu guo zhong guo ma", "你去过中国吗"),
    ("wo qu guo bei jing", "我去过北京"),
    ("shang hai shi yi ge da cheng shi", "上海是一个大城市"),
    ("zhong guo de li shi hen you jiu", "中国的历史很悠久"),
    ("wo men yao nu li xue xi", "我们要努力学习"),
    ("xue xi shi hen zhong yao de", "学习是很重要的"),
    ("qing ba men guan shang", "请把门关上"),
    ("ta zheng zai xie zuo ye", "他正在写作业"),
    ("wo men ming tian qu lv you", "我们明天去旅游"),
    ("zhe ben shu hen you yi si", "这本书很有意思"),
    ("ni de zhong wen shuo de hen hao", "你的中文说得很好"),
    ("wo zheng zai xue zhong wen", "我正在学中文"),
    ("ta zai da xue du shu", "他在大学读书"),
    ("zhe shi wo di yi ci lai zhong guo", "这是我第一次来中国"),
    ("huan ying ni lai zhong guo", "欢迎你来中国"),
    ("zhong guo shi yi ge wei da de guo jia", "中国是一个伟大的国家"),
    ("wo men ying gai bao hu huan jing", "我们应该保护环境"),
    ("ke xue ji shu shi di yi sheng chan li", "科学技术是第一生产力"),
    ("jiao yu shi guo jia de wei lai", "教育是国家的未来"),
    ("wo de meng xiang shi dang yi sheng", "我的梦想是当医生"),
    ("ta de gong zuo shi cheng xu yuan", "他的工作是程序员"),
    ("ren gong zhi neng gai bian le shi jie", "人工智能改变了世界"),
    ("hu lian wang rang shi jie geng xiao", "互联网让世界更小"),
    ("wo men yao re ai sheng huo", "我们要热爱生活"),
    ("jin tian de tian qi fei chang hao", "今天的天气非常好"),
    ("ming tian wo men qu gong yuan wan", "明天我们去公园玩"),
    ("ta shi yi ge hen you neng li de ren", "他是一个很有能力的人"),
    ("wo men ying gai hu xiang bang zhu", "我们应该互相帮助"),
    ("sheng huo zhong zui zhong yao de shi jian kang", "生活中最重要的是健康"),
    ("du shu ke yi zeng zhang zhi shi", "读书可以增长知识"),
    ("mei ge ren dou you zi ji de meng xiang", "每个人都有自己的梦想"),
    ("shi jian jiu shi jin qian", "时间就是金钱"),
    ("zhi shi gai bian ming yun", "知识改变命运"),
    ("tuan jie jiu shi li liang", "团结就是力量"),
    ("shi jie shang mei you mian fei de wu can", "世界上没有免费的午餐"),
    ("yi fen geng yun yi fen shou huo", "一分耕耘一分收获"),
    ("wo xiang ni le", "我想你了"),
    ("zhu ni sheng ri kuai le", "祝你生日快乐"),
    ("xin nian kuai le", "新年快乐"),
    ("zhong qiu jie kuai le", "中秋节快乐"),
    ("gong xi fa cai", "恭喜发财"),
    ("shun shun li li", "顺顺利利"),
    ("wan shi ru yi", "万事如意"),
    ("shen ti jian kang", "身体健康"),
    ("gong zuo shun li", "工作顺利"),
    ("xue ye you cheng", "学业有成"),
    ("wo xia ge yue qu lv you", "我下个月去旅游"),
    ("ta men zheng zai kai hui", "他们正在开会"),
    ("zhe jian yi fu hen hao kan", "这件衣服很好看"),
    ("ni neng bang wo yi xia ma", "你能帮我一下吗"),
    ("wo men yi qi qu ba", "我们一起去吧"),
    ("ta de cheng ji hen hao", "他的成绩很好"),
    ("yi hou wo yao geng jia nu li", "以后我要更加努力"),
    ("zhong guo ren kou zui duo", "中国人口最多"),
    ("bei jing shi zhong guo de shou du", "北京是中国的首都"),
    ("chang cheng shi shi jie qi ji", "长城是世界奇迹"),
    ("xi hu fei chang mei li", "西湖非常美丽"),
    ("zhong guo you wu qian nian de wen ming", "中国有五千年的文明"),
    # 科技主题句子
    ("ji qi xue xi shi ren gong zhi neng de fen zhi", "机器学习是人工智能的分支"),
    ("shen du xue xi yong yu tu xiang shi bie", "深度学习用于图像识别"),
    ("shu ju ku cun chu da liang shu ju", "数据库存储大量数据"),
    ("wo men xu yao geng duo de fu wu qi", "我们需要更多的服务器"),
    ("ruan jian kai fa xu yao tuan dui he zuo", "软件开发需要团队合作"),
    ("wang luo an quan fei chang zhong yao", "网络安全非常重要"),
    ("yun ji suan ti gong le qiang da de ji suan neng li", "云计算提供了强大的计算能力"),
    ("shou ji yi jing cheng wei bi xu pin", "手机已经成为必需品"),
    ("dian zi you jian shi chang yong de tong xin fang shi", "电子邮件是常用的通信方式"),
    # 日常更多
    ("wo qu chao shi mai dong xi", "我去超市买东西"),
    ("ta zai jia zuo fan", "他在家做饭"),
    ("wo men yi qi kan dian ying ba", "我们一起看电影吧"),
    ("zhe ge zhou mo ni you shi jian ma", "这个周末你有时间吗"),
    ("wo yao xue yi men xin de yu yan", "我要学一门新的语言"),
    ("ta zheng zai ting yin yue", "他正在听音乐"),
    ("wo xi huan da lan qiu", "我喜欢打篮球"),
    ("yun dong dui shen ti hen hao", "运动对身体很好"),
    ("jin tian de zuo ye hen duo", "今天的作业很多"),
    ("lao shi shuo de fei chang hao", "老师说得非常好"),
    ("wo men ban you san shi ge xue sheng", "我们班有三十个学生"),
    ("tu shu guan li you hen duo shu", "图书馆里有很多书"),
    ("yi yuan li you hen duo bing ren", "医院里有很多病人"),
    ("gong yuan li de hua hen mei li", "公园里的花很美丽"),
    ("chun tian de hua kai le", "春天的花开了"),
    ("xia tian hen re", "夏天很热"),
    ("qiu tian de ye zi huang le", "秋天的叶子黄了"),
    ("dong tian hen leng", "冬天很冷"),
    ("wo de jia zai bei jing", "我的家在北京"),
    ("wo mei tian zuo di tie shang ban", "我每天坐地铁上班"),
    ("ta mai le yi tai xin dian nao", "他买了一台新电脑"),
    ("zhe ge shou ji hen hao yong", "这个手机很好用"),
    ("wo men gong si you yi bai ge ren", "我们公司有一百个人"),
    ("ta de ying yu shuo de hen liu li", "他的英语说得很流利"),
    ("wo men yao bao chi le guan de xin tai", "我们要保持乐观的心态"),
    ("sheng huo xu yao yong qi he jue xin", "生活需要勇气和决心"),
]

# --- 成语 (4 字) ---
IDIOMS = [
    ("yi xin yi yi", "一心一意"),
    ("ren shan ren hai", "人山人海"),
    ("san xin er yi", "三心二意"),
    ("wu yan liu se", "五颜六色"),
    ("qi shang ba xia", "七上八下"),
    ("shi quan shi mei", "十全十美"),
    ("bai fa bai zhong", "百发百中"),
    ("qian jun wan ma", "千军万马"),
    ("wan zi qian hong", "万紫千红"),
    ("tian chang di jiu", "天长地久"),
    ("feng he ri li", "风和日丽"),
    ("hua hao yue yuan", "花好月圆"),
    ("xin hua nu fang", "心花怒放"),
    ("long fei feng wu", "龙飞凤舞"),
    ("hu shuo ba dao", "胡说八道"),
    ("zi li geng sheng", "自力更生"),
    ("gu rou xiang lian", "骨肉相连"),
    ("du yi wu er", "独一无二"),
    ("xing gao cai lie", "兴高采烈"),
    ("yi ma ping chuan", "一马平川"),
    ("tu qiong bi jian", "图穷匕见"),
    ("yi ming jing ren", "一鸣惊人"),
    ("jing tian dong di", "惊天动地"),
    ("wai qiang zhong gan", "外强中干"),
    ("wu niu chong tian", "五牛冲天"),
    ("ri xin yue yi", "日新月异"),
    ("tuo ying er chu", "脱颖而出"),
    ("ru hu tian yi", "如虎添翼"),
    ("qian li tiao tiao", "千里迢迢"),
    ("bu yue er tong", "不约而同"),
    ("ji shao cheng duo", "积少成多"),
    ("qin neng bu zhuo", "勤能补拙"),
    ("kai juan you yi", "开卷有益"),
    ("wen gu zhi xin", "温故知新"),
    ("xue wu zhi jing", "学无止境"),
    ("yin cai shi jiao", "因材施教"),
    ("bu chi xia wen", "不耻下问"),
    ("gong cheng ming jiu", "功成名就"),
    ("jin xiu qian cheng", "锦绣前程"),
    ("peng cheng wan li", "鹏程万里"),
]

# --- 2人字组合 ---
TWO_CHAR = [
    ("shi jie", "世界"),
    ("ren min", "人民"),
    ("guo jia", "国家"),
    ("she hui", "社会"),
    ("jing ji", "经济"),
    ("wen hua", "文化"),
    ("zheng zhi", "政治"),
    ("jun shi", "军事"),
    ("ke xue", "科学"),
    ("ji shu", "技术"),
    ("jiao yu", "教育"),
    ("ti yu", "体育"),
    ("yi shu", "艺术"),
    ("yin yue", "音乐"),
    ("dian ying", "电影"),
    ("li shi", "历史"),
    ("di li", "地理"),
    ("shu xue", "数学"),
    ("wu li", "物理"),
    ("hua xue", "化学"),
    ("sheng wu", "生物"),
    ("yi xue", "医学"),
    ("fa lv", "法律"),
    ("zhe xue", "哲学"),
    ("wen xue", "文学"),
    ("ren lei", "人类"),
    ("zi ran", "自然"),
    ("huan jing", "环境"),
    ("nong ye", "农业"),
    ("gong ye", "工业"),
    ("shang ye", "商业"),
    ("si fa", "司法"),
    ("min zu", "民族"),
    ("she ji", "设计"),
    ("guan li", "管理"),
    ("ji hua", "计划"),
    ("mu biao", "目标"),
    ("wen ti", "问题"),
    ("fang fa", "方法"),
    ("jie guo", "结果"),
    ("yuan yin", "原因"),
    ("qing kuang", "情况"),
    ("xi wang", "希望"),
    ("ji hui", "机会"),
    ("tiao jian", "条件"),
    ("xiao guo", "效果"),
    ("shui ping", "水平"),
    ("neng li", "能力"),
    ("jing yan", "经验"),
    ("ze ren", "责任"),
    ("ren wu", "任务"),
    ("biao zhun", "标准"),
    ("gui ze", "规则"),
    ("yuan ze", "原则"),
    ("zi you", "自由"),
    ("ping deng", "平等"),
    ("gong zheng", "公正"),
    ("he ping", "和平"),
    ("fa zhan", "发展"),
    ("jin bu", "进步"),
    ("chuang xin", "创新"),
    ("he zuo", "合作"),
    ("jing zheng", "竞争"),
    ("cheng gong", "成功"),
    ("shi bai", "失败"),
    ("xin fu", "幸福"),
    ("kuai le", "快乐"),
    ("jian kang", "健康"),
    ("an quan", "安全"),
    ("wen ding", "稳定"),
    ("su du", "速度"),
    ("li liang", "力量"),
    ("zhi hui", "智慧"),
    ("yong qi", "勇气"),
    ("xin xin", "信心"),
    ("nai xin", "耐心"),
    ("mei li", "美丽"),
    ("ke ai", "可爱"),
    ("wen nuan", "温暖"),
    ("liang kuai", "凉快"),
    ("qing lang", "晴朗"),
    ("xu yao", "需要"),
    ("ying gai", "应该"),
    ("ke yi", "可以"),
    ("bi xu", "必须"),
    ("yuan yi", "愿意"),
    ("xi huan", "喜欢"),
    ("fan dui", "反对"),
    ("tong yi", "同意"),
    ("peng you", "朋友"),
    ("jia ting", "家庭"),
    ("fu mu", "父母"),
    ("xiong di", "兄弟"),
    ("jie mei", "姐妹"),
    ("lin ju", "邻居"),
    ("fang zi", "房子"),
    ("jian zhu", "建筑"),
    ("dao lu", "道路"),
    ("qiao liang", "桥梁"),
    ("che liang", "车辆"),
    ("lv xing", "旅行"),
    ("du jia", "度假"),
    ("can ting", "餐厅"),
    ("jiu dian", "酒店"),
    ("yin hang", "银行"),
    ("yi yuan", "医院"),
    ("xue xiao", "学校"),
    ("gong chang", "工厂"),
    ("shi chang", "市场"),
    ("zheng fu", "政府"),
]

def validate(py_str, char_str):
    """验证拼音和汉字数量匹配"""
    syllables = py_str.strip().split()
    chars = list(char_str.strip())
    return len(syllables) == len(chars)


def augment_samples(base_samples):
    """数据增强: 拼接多条短语形成更长的训练样本"""
    augmented = []
    random.seed(42)

    # 两两拼接
    keys = list(base_samples)
    for _ in range(len(keys) * 2):
        a = random.choice(keys)
        b = random.choice(keys)
        if a == b:
            continue
        py_a, ch_a = a
        py_b, ch_b = b
        combined_py = py_a + " " + py_b
        combined_ch = ch_a + ch_b
        if validate(combined_py, combined_ch):
            augmented.append((combined_py, combined_ch))

    # 三个拼接 (更长文本)
    for _ in range(len(keys)):
        a = random.choice(keys)
        b = random.choice(keys)
        c = random.choice(keys)
        if a == b or b == c:
            continue
        combined_py = a[0] + " " + b[0] + " " + c[0]
        combined_ch = a[1] + b[1] + c[1]
        if validate(combined_py, combined_ch):
            augmented.append((combined_py, combined_ch))

    return augmented


def main():
    all_data = []

    # 收集所有基础数据
    for dataset in [DAILY, SENTENCES, IDIOMS, TWO_CHAR]:
        for py, ch in dataset:
            if validate(py, ch):
                all_data.append((py, ch))
            else:
                print(f"⚠ 长度不匹配, 跳过: {py} → {ch} "
                      f"({len(py.split())} vs {len(list(ch))})")

    base_count = len(all_data)
    print(f"基础样本: {base_count}")

    # 数据增强
    augmented = augment_samples(all_data)
    all_data.extend(augmented)
    print(f"增强后样本: {len(all_data)}")

    # 去重
    seen = set()
    unique = []
    for item in all_data:
        key = item[0] + "\t" + item[1]
        if key not in seen:
            seen.add(key)
            unique.append(item)
    all_data = unique
    print(f"去重后样本: {len(all_data)}")

    # 随机打乱
    random.shuffle(all_data)

    # 写入文件
    out_dir = os.path.join(os.path.dirname(__file__), "data")
    os.makedirs(out_dir, exist_ok=True)
    out_path = os.path.join(out_dir, "corpus.txt")

    with open(out_path, "w", encoding="utf-8") as f:
        for py, ch in all_data:
            f.write(f"{py}\t{ch}\n")

    print(f"\n✅ 已生成语料: {out_path}")
    print(f"   总样本数: {len(all_data)}")

    # 统计
    total_chars = sum(len(ch) for _, ch in all_data)
    avg_len = total_chars / len(all_data)
    max_len = max(len(ch) for _, ch in all_data)
    print(f"   总字符数: {total_chars}")
    print(f"   平均长度: {avg_len:.1f} 字")
    print(f"   最大长度: {max_len} 字")


if __name__ == "__main__":
    main()
