"""Render the project's geometric application icon. / 生成程序图标。"""
from pathlib import Path
from PIL import Image, ImageDraw

folder=Path(__file__).resolve().parent
icon=Image.new('RGBA',(256,256),(0,0,0,0))
draw=ImageDraw.Draw(icon)
draw.rounded_rectangle((0,0,255,255),radius=46,fill='#152438')
draw.polygon([(128,27),(150,106),(229,128),(150,150),(128,229),(106,150),(27,128),(106,106)],fill='#67d9e8')
draw.ellipse((50,50,66,66),fill='white')
draw.ellipse((183,179,207,203),fill='white')
icon.save(folder/'app.png')
icon.save(folder/'app.ico',sizes=[(16,16),(24,24),(32,32),(48,48),(64,64),(128,128),(256,256)])
