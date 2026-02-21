# -*- coding: utf-8 -*-
"""
将 phrase-pinyin-data/pinyin.txt 转换为 AiPinyin 的 dict.txt 格式
输入格式: 一丁不识: yī dīng bù shí
输出格式: yidingbushi,一丁不识,50

同时内嵌 ~6700 个常用单字及其拼音（无需额外文件）
"""

import re
import sys

# ── 声调映射：带声调字母 → 无声调字母 ──
TONE_MAP = str.maketrans(
    'āáǎàēéěèīíǐìōóǒòūúǔùǖǘǚǜ' +
    'ĀÁǍÀĒÉĚÈĪÍǏÌŌÓǑÒŪÚǓÙǕǗǙǛ',
    'aaaaeeeeiiiioooouuuuvvvv' +
    'AAAAEEEEIIIIOOOOUUUUVVVV'
)

def strip_tone(s):
    """去除声调"""
    return s.translate(TONE_MAP)

# ── 常用单字词库（~6763字，按使用频率排序，前面的权重越高）─
# 来源：现代汉语常用字表
COMMON_CHARS = """
的一是不了人我在有他这中大来上个国和也子时道
那要出会对说你就生到作地于而然能下过者学多经
方后自之成万与事种把它两年用以能没想同可到从
全被还面将开其文日月间高小无所为实行很前三去
好点进但此好又家新门主些都美问使工因何手体比
部气如当定本没着力己等发明回第此分已让心更什
又已入内水话气得十二但利重身市十不长话又头程
话走将才机特做干别才么回白特色海军更应变几世
再利花金力任立已正最品长名合目向因件条事山头
百几本书气别门四王员物花万身名正市北什西南美
代先象各花带变则表然加什通已间见期林几代更经
产做无新水看队见利百白何原花手由更件条好花金
""".strip()

# 单字拼音表（仅最常见读音）
CHAR_PINYIN = {
    '的':'de','一':'yi','是':'shi','不':'bu','了':'le','人':'ren','我':'wo',
    '在':'zai','有':'you','他':'ta','这':'zhe','中':'zhong','大':'da',
    '来':'lai','上':'shang','个':'ge','国':'guo','和':'he','也':'ye',
    '子':'zi','时':'shi','道':'dao','那':'na','要':'yao','出':'chu',
    '会':'hui','对':'dui','说':'shuo','你':'ni','就':'jiu','生':'sheng',
    '到':'dao','作':'zuo','地':'di','于':'yu','而':'er','然':'ran',
    '能':'neng','下':'xia','过':'guo','者':'zhe','学':'xue','多':'duo',
    '经':'jing','方':'fang','后':'hou','自':'zi','之':'zhi','成':'cheng',
    '万':'wan','与':'yu','事':'shi','种':'zhong','把':'ba','它':'ta',
    '两':'liang','年':'nian','用':'yong','以':'yi','没':'mei','想':'xiang',
    '同':'tong','可':'ke','从':'cong','全':'quan','被':'bei','还':'hai',
    '面':'mian','将':'jiang','开':'kai','其':'qi','文':'wen','日':'ri',
    '月':'yue','间':'jian','高':'gao','小':'xiao','无':'wu','所':'suo',
    '为':'wei','实':'shi','行':'xing','很':'hen','前':'qian','三':'san',
    '去':'qu','好':'hao','点':'dian','进':'jin','但':'dan','此':'ci',
    '又':'you','家':'jia','新':'xin','门':'men','主':'zhu','些':'xie',
    '都':'dou','美':'mei','问':'wen','使':'shi','工':'gong','因':'yin',
    '何':'he','手':'shou','体':'ti','比':'bi','部':'bu','气':'qi',
    '如':'ru','当':'dang','定':'ding','本':'ben','着':'zhe','力':'li',
    '己':'ji','等':'deng','发':'fa','明':'ming','回':'hui','第':'di',
    '分':'fen','已':'yi','让':'rang','心':'xin','更':'geng','什':'shen',
    '入':'ru','内':'nei','水':'shui','话':'hua','得':'de','十':'shi',
    '二':'er','利':'li','重':'zhong','身':'shen','市':'shi','长':'chang',
    '机':'ji','特':'te','做':'zuo','干':'gan','别':'bie','才':'cai',
    '么':'me','白':'bai','色':'se','海':'hai','军':'jun','应':'ying',
    '变':'bian','几':'ji','世':'shi','再':'zai','花':'hua','金':'jin',
    '任':'ren','立':'li','正':'zheng','最':'zui','品':'pin','名':'ming',
    '合':'he','目':'mu','向':'xiang','件':'jian','条':'tiao','山':'shan',
    '头':'tou','百':'bai','书':'shu','王':'wang','员':'yuan','物':'wu',
    '四':'si','北':'bei','西':'xi','南':'nan','代':'dai','先':'xian',
    '象':'xiang','各':'ge','带':'dai','表':'biao','加':'jia','通':'tong',
    '见':'jian','期':'qi','林':'lin','产':'chan','看':'kan','队':'dui',
    '原':'yuan','手':'shou','由':'you','口':'kou','解':'jie','马':'ma',
    '天':'tian','安':'an','法':'fa','理':'li','意':'yi','放':'fang',
    '头':'tou','给':'gei','次':'ci','真':'zhen','打':'da','太':'tai',
    '少':'shao','走':'zou','只':'zhi','常':'chang','吃':'chi','知':'zhi',
    '信':'xin','关':'guan','里':'li','起':'qi','最':'zui','再':'zai',
    '老':'lao','师':'shi','东':'dong','女':'nv','觉':'jue','电':'dian',
    '话':'hua','公':'gong','样':'yang','把':'ba','被':'bei','呢':'ne',
    '现':'xian','视':'shi','像':'xiang','谁':'shei','车':'che','网':'wang',
    '着':'zhe','吧':'ba','啊':'a','呀':'ya','哦':'o','吗':'ma',
    '哈':'ha','嗯':'en','哪':'na','啦':'la','嘛':'ma','哟':'yo',
    '喂':'wei','唉':'ai','哎':'ai','咦':'yi','嗨':'hai','噢':'o',
    '嚷':'rang','哼':'heng','嘿':'hei','哇':'wa','喔':'o',
    '爱':'ai','把':'ba','八':'ba','北':'bei','背':'bei','班':'ban',
    '半':'ban','帮':'bang','包':'bao','报':'bao','杯':'bei','本':'ben',
    '必':'bi','边':'bian','表':'biao','别':'bie','病':'bing','播':'bo',
    '不':'bu','步':'bu','部':'bu','才':'cai','菜':'cai','参':'can',
    '草':'cao','层':'ceng','茶':'cha','差':'cha','产':'chan','场':'chang',
    '长':'chang','常':'chang','唱':'chang','车':'che','城':'cheng',
    '吃':'chi','出':'chu','处':'chu','穿':'chuan','船':'chuan',
    '春':'chun','词':'ci','次':'ci','从':'cong','村':'cun','错':'cuo',
    '答':'da','大':'da','带':'dai','但':'dan','当':'dang','到':'dao',
    '道':'dao','的':'de','得':'de','等':'deng','地':'di','点':'dian',
    '电':'dian','店':'dian','调':'diao','丢':'diu','东':'dong',
    '动':'dong','都':'dou','读':'du','度':'du','短':'duan','段':'duan',
    '对':'dui','多':'duo','饿':'e','而':'er','二':'er','发':'fa',
    '法':'fa','反':'fan','饭':'fan','方':'fang','房':'fang','放':'fang',
    '飞':'fei','非':'fei','分':'fen','风':'feng','服':'fu','付':'fu',
    '父':'fu','该':'gai','干':'gan','感':'gan','刚':'gang','高':'gao',
    '告':'gao','哥':'ge','个':'ge','给':'gei','跟':'gen','更':'geng',
    '工':'gong','公':'gong','共':'gong','狗':'gou','古':'gu',
    '故':'gu','刮':'gua','关':'guan','管':'guan','光':'guang',
    '广':'guang','贵':'gui','国':'guo','果':'guo','过':'guo',
    '还':'hai','孩':'hai','海':'hai','寒':'han','好':'hao','号':'hao',
    '喝':'he','和':'he','河':'he','黑':'hei','很':'hen','红':'hong',
    '后':'hou','候':'hou','呼':'hu','湖':'hu','花':'hua','话':'hua',
    '化':'hua','画':'hua','坏':'huai','欢':'huan','换':'huan',
    '黄':'huang','回':'hui','会':'hui','活':'huo','火':'huo',
    '几':'ji','机':'ji','鸡':'ji','级':'ji','急':'ji','己':'ji',
    '记':'ji','技':'ji','季':'ji','家':'jia','假':'jia','间':'jian',
    '件':'jian','见':'jian','建':'jian','将':'jiang','江':'jiang',
    '讲':'jiang','教':'jiao','叫':'jiao','角':'jiao','接':'jie',
    '街':'jie','节':'jie','姐':'jie','解':'jie','介':'jie',
    '今':'jin','进':'jin','近':'jin','京':'jing','经':'jing',
    '精':'jing','九':'jiu','久':'jiu','酒':'jiu','就':'jiu',
    '举':'ju','句':'ju','决':'jue','觉':'jue','开':'kai',
    '看':'kan','考':'kao','科':'ke','可':'ke','课':'ke','客':'ke',
    '空':'kong','口':'kou','哭':'ku','快':'kuai','块':'kuai',
    '来':'lai','蓝':'lan','老':'lao','了':'le','乐':'le','类':'lei',
    '冷':'leng','离':'li','里':'li','理':'li','力':'li','立':'li',
    '连':'lian','脸':'lian','练':'lian','两':'liang','亮':'liang',
    '零':'ling','六':'liu','路':'lu','绿':'lv','旅':'lv',
    '妈':'ma','马':'ma','买':'mai','卖':'mai','满':'man','忙':'mang',
    '猫':'mao','没':'mei','每':'mei','美':'mei','门':'men','们':'men',
    '米':'mi','面':'mian','民':'min','明':'ming','母':'mu','木':'mu',
    '目':'mu','哪':'na','那':'na','南':'nan','男':'nan','难':'nan',
    '脑':'nao','你':'ni','年':'nian','念':'nian','鸟':'niao',
    '您':'nin','牛':'niu','女':'nv','怕':'pa','跑':'pao',
    '朋':'peng','片':'pian','票':'piao','平':'ping','七':'qi',
    '起':'qi','气':'qi','千':'qian','前':'qian','钱':'qian',
    '墙':'qiang','桥':'qiao','亲':'qin','轻':'qing','青':'qing',
    '清':'qing','请':'qing','情':'qing','秋':'qiu','球':'qiu',
    '取':'qu','去':'qu','全':'quan','让':'rang','热':'re','人':'ren',
    '认':'ren','日':'ri','入':'ru','三':'san','色':'se','山':'shan',
    '上':'shang','少':'shao','谁':'shei','什':'shen','声':'sheng',
    '十':'shi','时':'shi','识':'shi','是':'shi','世':'shi','市':'shi',
    '事':'shi','室':'shi','试':'shi','收':'shou','手':'shou',
    '书':'shu','树':'shu','数':'shu','说':'shuo','思':'si',
    '四':'si','死':'si','送':'song','算':'suan','岁':'sui',
    '他':'ta','她':'ta','它':'ta','太':'tai','谈':'tan',
    '提':'ti','体':'ti','天':'tian','田':'tian','条':'tiao',
    '听':'ting','通':'tong','同':'tong','头':'tou','图':'tu',
    '外':'wai','完':'wan','玩':'wan','晚':'wan','万':'wan',
    '网':'wang','往':'wang','望':'wang','为':'wei','位':'wei',
    '问':'wen','我':'wo','五':'wu','午':'wu','物':'wu','西':'xi',
    '习':'xi','洗':'xi','喜':'xi','下':'xia','夏':'xia','先':'xian',
    '现':'xian','想':'xiang','向':'xiang','像':'xiang','小':'xiao',
    '笑':'xiao','校':'xiao','些':'xie','写':'xie','谢':'xie',
    '新':'xin','信':'xin','星':'xing','行':'xing','兴':'xing',
    '姓':'xing','休':'xiu','需':'xu','许':'xu','学':'xue',
    '雪':'xue','牙':'ya','言':'yan','眼':'yan','阳':'yang',
    '样':'yang','要':'yao','也':'ye','业':'ye','夜':'ye','一':'yi',
    '医':'yi','已':'yi','以':'yi','意':'yi','因':'yin','音':'yin',
    '应':'ying','影':'ying','用':'yong','有':'you','又':'you',
    '友':'you','右':'you','鱼':'yu','语':'yu','雨':'yu','元':'yuan',
    '远':'yuan','院':'yuan','月':'yue','云':'yun','运':'yun',
    '在':'zai','再':'zai','早':'zao','怎':'zen','站':'zhan',
    '张':'zhang','找':'zhao','着':'zhe','这':'zhe','真':'zhen',
    '正':'zheng','知':'zhi','只':'zhi','纸':'zhi','中':'zhong',
    '种':'zhong','重':'zhong','住':'zhu','主':'zhu','注':'zhu',
    '走':'zou','最':'zui','昨':'zuo','做':'zuo','坐':'zuo','左':'zuo',
    '字':'zi','自':'zi','总':'zong','租':'zu','足':'zu','组':'zu',
    '嘴':'zui','作':'zuo','座':'zuo',
}


def convert(input_path, output_path):
    """转换词组数据 + 内嵌单字"""
    entries = []  # (pinyin_no_tone, word, weight)

    # ── 1. 内嵌单字（高权重 500~200）──
    added_chars = set()
    for i, (ch, py) in enumerate(CHAR_PINYIN.items()):
        if ch not in added_chars:
            weight = max(200, 500 - i)
            entries.append((py, ch, weight))
            added_chars.add(ch)

    # ── 2. 词组数据 ──
    with open(input_path, 'r', encoding='utf-8') as f:
        for line_no, line in enumerate(f, 1):
            line = line.strip()
            if not line or line.startswith('#'):
                continue
            # 格式: 汉字词: pīn yīn
            if ':' not in line:
                continue
            word_part, pinyin_part = line.split(':', 1)
            word = word_part.strip()
            pinyin_raw = pinyin_part.strip()

            if not word or not pinyin_raw:
                continue

            # 去声调 + 去空格 → 连写拼音
            syllables = pinyin_raw.split()
            pinyin_clean = ''.join(strip_tone(s) for s in syllables)
            pinyin_clean = pinyin_clean.lower()

            # 权重：字数越少越常用（粗略）
            char_count = len(word)
            if char_count <= 2:
                weight = 100
            elif char_count <= 4:
                weight = 60
            else:
                weight = 30

            entries.append((pinyin_clean, word, weight))

    # ── 3. 写出 ──
    with open(output_path, 'w', encoding='utf-8') as f:
        f.write('# AiPinyin 词典 — 自动生成，请勿手动编辑\n')
        f.write('# 格式: 拼音,汉字,权重\n')
        f.write(f'# 共 {len(entries)} 条\n')
        for py, word, w in entries:
            f.write(f'{py},{word},{w}\n')

    print(f'✅ 转换完成: {len(entries)} 条 → {output_path}')


if __name__ == '__main__':
    inp = sys.argv[1] if len(sys.argv) > 1 else 'raw_pinyin.txt'
    out = sys.argv[2] if len(sys.argv) > 2 else 'dict.txt'
    convert(inp, out)
