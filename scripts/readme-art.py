import random, math
import os
OUT=os.path.join(os.path.dirname(os.path.abspath(__file__)),'..','docs','readme')+os.sep
C=dict(chrome='#142227',paper='#182930',paper2='#1F323A',paper3='#283D45',paper4='#324750',ink='#F1ECE3',ink2='#D2CDC2',muted='#A0B2B4',rule='#35474E',edge='#6A7E85',accent='#7FE0C7',accentink='#002826',wash='#1D4841',seal='#F6B070',sealink='#3A1E0E')
FD="'Segoe UI Variable Display','Segoe UI',Inter,system-ui,-apple-system,Helvetica,Arial,sans-serif"
FB="'Segoe UI Variable Text','Segoe UI',Inter,system-ui,-apple-system,Helvetica,Arial,sans-serif"
FM="'Cascadia Code','Cascadia Mono',ui-monospace,Consolas,Menlo,monospace"

def bird(p, open_=0.0):
    a=open_*12
    return f'''<defs>
<linearGradient id="{p}c" x1="96" y1="30" x2="173" y2="166" gradientUnits="userSpaceOnUse"><stop stop-color="#47CEC9"/><stop offset=".48" stop-color="#189BB4"/><stop offset="1" stop-color="#13618E"/></linearGradient>
<linearGradient id="{p}b" x1="141" y1="102" x2="157" y2="210" gradientUnits="userSpaceOnUse"><stop stop-color="#FBC782"/><stop offset=".5" stop-color="#EEA15B"/><stop offset="1" stop-color="#D57538"/></linearGradient>
<linearGradient id="{p}w" x1="79" y1="105" x2="105" y2="194" gradientUnits="userSpaceOnUse"><stop stop-color="#288FB4"/><stop offset="1" stop-color="#164968"/></linearGradient>
<linearGradient id="{p}k" x1="196" y1="62" x2="196" y2="83" gradientUnits="userSpaceOnUse"><stop stop-color="#88A5AE"/><stop offset=".48" stop-color="#4E7285"/><stop offset=".52" stop-color="#2D4B61"/></linearGradient>
</defs>
<path fill="#245D78" d="M62 169L97 195L36 225L19 220Z"/>
<path d="M98 201L93 224L105 228M93 224L86 227M123 201L118 224L131 227M118 224L111 228" fill="none" stroke="#AA6346" stroke-width="4" stroke-linecap="round" stroke-linejoin="round"/>
<path fill="url(#{p}k)" d="M173 62L246 74L173 72Z" transform="rotate({-a} 173 72)"/>
<path fill="url(#{p}k)" d="M173 72L246 74L173 82Z" transform="rotate({a} 173 72)"/>
<path fill="url(#{p}c)" d="M34 197C43 158 63 114 87 86C85 70 89 50 104 38C120 27 143 27 160 35C177 43 183 60 177 77L169 92C179 108 177 134 164 158C146 193 124 213 99 211C73 211 49 198 34 197Z"/>
<path fill="url(#{p}b)" d="M169 89C179 107 177 133 164 158C146 193 124 213 99 211C87 211 76 207 67 202C91 187 108 166 120 141C130 118 145 100 169 89Z"/>
<path fill="#F8EDCF" d="M170 88C175 97 177 111 174 122C165 119 158 111 154 102C160 97 165 92 170 88Z"/>
<path fill="#D98A48" d="M114 71C128 78 143 78 156 73L163 85C150 97 135 99 122 91Z"/>
<path fill="#F6EDD8" d="M111 78L124 92L115 102C109 99 104 93 103 89Z"/>
<path fill="#5BDFCE" d="M92 95C72 124 56 157 42 190L34 197C44 157 66 116 86 91Z"/>
<path fill="url(#{p}w)" d="M123 99C105 90 85 108 69 131C53 154 42 177 34 197C65 193 94 174 114 148C128 129 134 111 123 99Z"/>
<path d="M122 110C110 135 88 157 61 179M119 129C105 151 86 166 69 175" fill="none" stroke="#6EC9CD" stroke-width="2" stroke-linecap="round" opacity=".35"/>
<path d="M104 46C119 35 142 35 155 42" fill="none" stroke="#A0E3D8" stroke-width="3" stroke-linecap="round" opacity=".55"/>
<circle fill="#102936" cx="155" cy="60" r="5.5"/><circle fill="#F7F2DE" cx="153.5" cy="58.5" r="1.4"/>'''

def keycap(x,y,label,w,h=44):
    return (f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="7" fill="{C["paper3"]}" stroke="{C["edge"]}"/>'
            f'<rect x="{x+1}" y="{y+h-5}" width="{w-2}" height="4" rx="2" fill="{C["paper"]}" opacity=".55"/>'
            f'<text x="{x+w/2}" y="{y+h/2+6}" text-anchor="middle" font-family="{FB}" font-size="17" font-weight="600" fill="{C["ink"]}">{label}</text>')

def wave(x0,y,n,step,amp,color,seed):
    random.seed(seed); out=[]
    for i in range(n):
        env=math.sin(math.pi*(i+0.5)/n)**0.7
        hgt=max(4, amp*env*(0.35+0.65*abs(math.sin(i*0.9+seed))*random.uniform(.6,1)))
        out.append(f'<rect x="{x0+i*step:.1f}" y="{y-hgt/2:.1f}" width="{step*0.55:.1f}" height="{hgt:.1f}" rx="{step*0.27:.1f}" fill="{color}"/>')
    return ''.join(out)

# ---------- hero ----------
W,H=1280,520
s=[f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}" role="img" aria-labelledby="t d">',
   '<title id="t">Whisper Bird</title><desc id="d">Lokales Diktieren für Windows. Strg + Umschalt + Leertaste drücken, sprechen, der Text erscheint am Cursor.</desc>',
   f'<rect width="{W}" height="{H}" rx="20" fill="{C["paper"]}"/>',
   # sidebar-like chrome strip on the left
   f'<path d="M20 0H300V{H}H20A20 20 0 0 1 0 {H-20}V20A20 20 0 0 1 20 0Z" fill="{C["chrome"]}"/>',
   f'<g transform="translate(46 132) scale(0.86)">{bird("h")}</g>',
   f'<line x1="300" y1="0" x2="300" y2="{H}" stroke="{C["rule"]}"/>',
   # copy
   f'<text x="360" y="150" font-family="{FB}" font-size="16" font-weight="600" letter-spacing="2.2" fill="{C["accent"]}">LOKAL · WINDOWS · IN JEDE APP</text>',
   f'<text x="356" y="222" font-family="{FD}" font-size="68" font-weight="600" letter-spacing="-1.4" fill="{C["ink"]}">Whisper Bird</text>',
   f'<text font-family="{FB}" font-size="23" fill="{C["ink2"]}"><tspan x="360" y="266">Sprechen statt tippen.</tspan><tspan x="360" y="296">Der Text landet dort, wo der Cursor steht.</tspan></text>',
]
# keycaps
x=360;y=332
for lab,w in [('Strg',78),('Umschalt',122),('Leertaste',186)]:
    s.append(keycap(x,y,lab,w)); x+=w
    if lab!='Leertaste':
        s.append(f'<text x="{x+15}" y="{y+29}" text-anchor="middle" font-family="{FB}" font-size="20" fill="{C["muted"]}">+</text>'); x+=30
# recording pill (overlay)
px,py,pw,ph=360,412,480,62
s+= [f'<rect x="{px}" y="{py}" width="{pw}" height="{ph}" rx="31" fill="{C["chrome"]}" stroke="{C["rule"]}"/>',
     f'<g transform="translate({px+12} {py+7}) scale(0.19)">{bird("p",0.7)}</g>',
     wave(px+78,py+ph/2,32,8.2,34,C['accent'],3),
     f'<circle cx="{px+pw-108}" cy="{py+ph/2}" r="6" fill="{C["seal"]}"/>',
     f'<text x="{px+pw-94}" y="{py+ph/2+6}" font-family="{FM}" font-size="16" fill="{C["ink2"]}">00:07</text>',
     f'<circle cx="{px+pw-22}" cy="{py+ph/2}" r="12" fill="{C["paper3"]}"/>',
     f'<path d="M{px+pw-27} {py+ph/2-5}l10 10m0-10l-10 10" stroke="{C["muted"]}" stroke-width="2" stroke-linecap="round"/>']
# right side: typed result card
cx,cy,cw,ch=920,292,310,182
s+= [f'<rect x="{cx}" y="{cy}" width="{cw}" height="{ch}" rx="14" fill="{C["paper2"]}"/>',
     f'<text x="{cx+22}" y="{cy+36}" font-family="{FB}" font-size="14" font-weight="600" letter-spacing="1.4" fill="{C["muted"]}">OUTLOOK · E-MAIL</text>',
     f'<line x1="{cx}" y1="{cy+54}" x2="{cx+cw}" y2="{cy+54}" stroke="{C["rule"]}"/>',
     f'<text font-family="{FB}" font-size="17" fill="{C["ink"]}"><tspan x="{cx+22}" y="{cy+92}">Hallo Jana,</tspan><tspan x="{cx+22}" y="{cy+122}">passt dir Donnerstag um zehn?</tspan><tspan x="{cx+22}" y="{cy+152}">Viele Grüße</tspan></text>',
     f'<rect x="{cx+122}" y="{cy+136}" width="2" height="21" fill="{C["accent"]}"/>',
     # enhance chip
     f'<rect x="{cx+cw-128}" y="{cy-18}" width="112" height="34" rx="17" fill="{C["wash"]}"/>',
     f'<path d="M{cx+cw-106} {cy-1}l4-9 4 9 9 4-9 4-4 9-4-9-9-4z" fill="{C["accent"]}" transform="translate(0 -4) scale(1)" />',
     f'<text x="{cx+cw-86}" y="{cy+5}" font-family="{FB}" font-size="15" font-weight="600" fill="{C["accent"]}">Enhance</text>']
# small model badges top right
bx=920
for lab in ['Parakeet','Whisper','Gemma 4']:
    w=len(lab)*9.6+30
    s.append(f'<rect x="{bx}" y="118" width="{w}" height="32" rx="16" fill="none" stroke="{C["rule"]}"/>'
             f'<text x="{bx+w/2}" y="139" text-anchor="middle" font-family="{FB}" font-size="15" fill="{C["muted"]}">{lab}</text>')
    bx+=w+10
s.append(f'<text x="920" y="186" font-family="{FB}" font-size="15" fill="{C["muted"]}">Modelle laufen auf deinem Rechner,</text>')
s.append(f'<text x="920" y="208" font-family="{FB}" font-size="15" fill="{C["muted"]}">beschleunigt per Vulkan-GPU.</text>')
s.append('</svg>')
open(OUT+'hero.svg','w').write('\n'.join(s))

# ---------- flow ----------
W,H=1280,240
steps=[('1','Aufnehmen','Mikrofon, PC-Audio','oder Datei'),('2','Erkennen','Parakeet, Qwen3, Canary,','Whisper – lokal'),('3','Verbessern','Enhance: Grammatik,','Ton, Format'),('4','Einfügen','an der Cursorposition,','Zwischenablage bleibt')]
f=[f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}" role="img" aria-labelledby="t d">',
   '<title id="t">Ablauf</title><desc id="d">Aufnehmen, Erkennen, Verbessern, Einfügen.</desc>',
   f'<rect width="{W}" height="{H}" rx="20" fill="{C["paper"]}"/>']
cw=268;gap=(W-80-4*cw)/3
for i,(n,t,a,b) in enumerate(steps):
    x=40+i*(cw+gap); y=36
    f.append(f'<rect x="{x}" y="{y}" width="{cw}" height="168" rx="14" fill="{C["paper2"]}"/>')
    hl= i==2
    f.append(f'<circle cx="{x+40}" cy="{y+44}" r="18" fill="{C["accent"] if hl else C["paper3"]}"/>'
             f'<text x="{x+40}" y="{y+50}" text-anchor="middle" font-family="{FB}" font-size="17" font-weight="600" fill="{C["accentink"] if hl else C["ink"]}">{n}</text>')
    f.append(f'<text x="{x+24}" y="{y+104}" font-family="{FD}" font-size="25" font-weight="600" letter-spacing="-.4" fill="{C["ink"]}">{t}</text>')
    f.append(f'<text font-family="{FB}" font-size="15" fill="{C["muted"]}"><tspan x="{x+24}" y="{y+132}">{a}</tspan><tspan x="{x+24}" y="{y+152}">{b}</tspan></text>')
    if i<3:
        ax=x+cw+gap/2
        f.append(f'<path d="M{ax-8} {y+84}h14m-5-6 6 6-6 6" fill="none" stroke="{C["edge"]}" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>')
f.append('</svg>')
open(OUT+'flow.svg','w').write('\n'.join(f))

# ---------- small bird mark for headings ----------
open(OUT+'bird.svg','w').write(f'<svg xmlns="http://www.w3.org/2000/svg" width="96" height="96" viewBox="0 0 256 256">{bird("m")}</svg>')
