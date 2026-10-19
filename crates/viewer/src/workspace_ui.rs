//! Lightweight shared PRO panel host; no scientific/controller state lives here.
//! 轻量停靠容器，只管理布局；收起/浮动不重建科学任务或复制其状态。
use eframe::egui::{self, Pos2, Rect, Sense, Stroke, Vec2};
use std::collections::{BTreeMap,BTreeSet};
use crate::i18n::t;

pub(crate) const CONTROL: f32 = 24.;
const GAP: f32 = 6.;
const SNAP: f32 = 24.;
// Layout keys include a task suffix; retain that full key for independent sizes.
// 布局键含子任务后缀；默认值按模块判断，独立尺寸仍保留完整键。
fn default_tool_height(operation:&str)->f32{if operation.split(':').next()==Some("archive_search"){600.}else{300.}}
fn saved_rect(value:&serde_json::Value)->Option<Rect>{let values=value.as_array().filter(|v|v.len()==4)?;let numbers=values.iter().map(|v|v.as_f64().filter(|n|n.is_finite() && n.abs()<1e6).map(|n|n as f32)).collect::<Option<Vec<_>>>()?;let rect=Rect::from_min_max(egui::pos2(numbers[0],numbers[1]),egui::pos2(numbers[2],numbers[3]));(rect.width()>=180. && rect.height()>=24.).then_some(rect)}
fn saved_size(value:&serde_json::Value)->Option<Vec2>{let values=value.as_array().filter(|v|v.len()==2)?;let size=egui::vec2(values[0].as_f64()? as f32,values[1].as_f64()? as f32);size.is_finite().then_some(egui::vec2(size.x.clamp(220.,2400.),size.y.clamp(96.,2400.)))}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Page { Tool, Coordinates, Photometry, Growth, Preview, Files, Secondary, Histogram, Settings, Header }
impl Page {
    pub const ALL: [Self; 10] = [Self::Tool,Self::Coordinates,Self::Photometry,Self::Growth,Self::Preview,Self::Files,Self::Secondary,Self::Histogram,Self::Settings,Self::Header];
    pub fn title(self)->&'static str {match self {
        Self::Tool=>"Current tool",Self::Coordinates=>"Coordinates",Self::Photometry=>"Aperture photometry",Self::Growth=>"Aperture growth curve",Self::Preview=>"Result preview",Self::Files=>"Add multiple files",Self::Secondary=>"Secondary view",Self::Histogram=>"Histogram",Self::Settings=>"Viewer tools",Self::Header=>"FITS header",
    }}
    fn short(self)->&'static str {match self {Self::Tool=>"Fx",Self::Coordinates=>"RA",Self::Photometry=>"A",Self::Growth=>"G",Self::Preview=>"Pv",Self::Files=>"F",Self::Secondary=>"2",Self::Histogram=>"H",Self::Settings=>"S",Self::Header=>"i"}}
    fn from_id(id:&str)->Option<Self>{Self::ALL.into_iter().find(|page|format!("{page:?}")==id)}
}

#[derive(Clone, Copy)]
pub(crate) enum Icon { Up, Down, More, Fold, Expand, Float, Dock, Close }

/// All actions use the same hit box and geometric center, independent of font baseline.
pub(crate) fn icon_button(ui:&mut egui::Ui,icon:Icon,label:&str)->egui::Response {
    let response=ui.allocate_response(Vec2::splat(CONTROL.max(ui.spacing().interact_size.y)),Sense::click());
    let visuals=ui.style().interact(&response);
    ui.painter().rect_filled(response.rect,3.,visuals.weak_bg_fill);
    ui.painter().rect_stroke(response.rect,3.,visuals.bg_stroke,egui::StrokeKind::Inside);
    let center=response.rect.center();let stroke=Stroke::new(1.5_f32,visuals.fg_stroke.color);
    let segment=|a:Vec2,b:Vec2|{ui.painter().line_segment([center+a,center+b],stroke);};
    match icon {
        Icon::Up|Icon::Down=>{let d=if matches!(icon,Icon::Up){-1.}else{1.};segment(egui::vec2(0.,-5.*d),egui::vec2(0.,5.*d));segment(egui::vec2(-4.,d),egui::vec2(0.,5.*d));segment(egui::vec2(0.,5.*d),egui::vec2(4.,d));},
        Icon::Fold=>{segment(egui::vec2(-4.,-2.),egui::vec2(0.,2.));segment(egui::vec2(0.,2.),egui::vec2(4.,-2.));},
        Icon::Expand=>{segment(egui::vec2(-2.,-4.),egui::vec2(2.,0.));segment(egui::vec2(2.,0.),egui::vec2(-2.,4.));},
        Icon::More=>{for x in [-4.,0.,4.]{ui.painter().circle_filled(center+egui::vec2(x,0.),1.2,stroke.color);}},
        Icon::Close=>{segment(egui::vec2(-3.5,-3.5),egui::vec2(3.5,3.5));segment(egui::vec2(-3.5,3.5),egui::vec2(3.5,-3.5));},
        Icon::Float|Icon::Dock=>{let rect=Rect::from_center_size(center,egui::vec2(11.,10.));ui.painter().rect_stroke(rect,1.,stroke,egui::StrokeKind::Inside);segment(egui::vec2(-5.5,-2.),egui::vec2(5.5,-2.));if matches!(icon,Icon::Dock){segment(egui::vec2(1.,-2.),egui::vec2(1.,5.));}else{segment(egui::vec2(1.,1.),egui::vec2(6.,-4.));}},
    }
    response.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,ui.is_enabled(),label));
    response.on_hover_text(label)
}

pub(crate) fn information(ui:&mut egui::Ui,id:impl std::hash::Hash,body:impl FnOnce(&mut egui::Ui)) {
    ui.push_id(id,|ui|{
        let response=icon_button(ui,Icon::More,t("Info"));
        egui::Popup::menu(&response).style(crate::ui_style::menu_style).show(|ui|{ui.set_max_width(360.);body(ui);});
    });
}

/// Center on the complete frame, reserving the larger action strip on both sides.
/// 标题以整个边框为中心，两侧对称预留按钮空间，窄窗时省略而不覆盖按钮。
fn centered_heading(ui:&mut egui::Ui,title:&str,side_inset:f32,sense:Sense)->(Rect,egui::Response) {
    let row=ui.allocate_response(egui::vec2(ui.available_width(),CONTROL.max(ui.spacing().interact_size.y)),Sense::hover());
    let width=(row.rect.width()-2.*side_inset).max(0.);
    let title_rect=Rect::from_center_size(row.rect.center(),egui::vec2(width,row.rect.height()));
    let response=ui.interact(title_rect,row.id.with("title"),sense);
    response.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Label,ui.is_enabled(),title));
    if width>2. && ui.is_rect_visible(title_rect) {
        let font=crate::ui_style::title_font(ui.style());
        let galley=egui::WidgetText::from(egui::RichText::new(title).font(font)).into_galley(ui,Some(egui::TextWrapMode::Truncate),width-2.,egui::TextStyle::Button);
        let position=title_rect.center()-galley.size()*0.5;
        let painter=ui.painter_at(title_rect);let color=ui.visuals().strong_text_color();
        crate::ui_style::paint_weighted_title(&painter,position,galley,color);
    }
    (row.rect,response.on_hover_text(title))
}

#[derive(Clone)]
struct Panel { floating:Option<u64>, folded:bool, height:f32, open:bool,last_rect:Option<Rect>,last_size:Option<Vec2> }
impl Default for Panel {fn default()->Self{Self{floating:None,folded:false,height:300.,open:false,last_rect:None,last_size:None}}}
struct Group {id:u64,pages:Vec<Page>,active:Page,rect:Rect,size:Vec2,position:Option<Pos2>,folded:bool,fresh:bool}
#[derive(Clone, Copy)]
struct Drag {group:u64,manual:Option<Vec2>,origin:Origin}
#[derive(Clone, Copy)]
enum Origin {Dock(usize),Floating(Rect)}
#[derive(Clone, Copy)]
struct Press {group:u64,origin:Pos2,rect:Rect}
#[derive(Clone, Copy)]
enum Target {Dock(usize),Group(u64),Align(Rect)}

/// Snap adjacent edges while retaining independent windows; title overlap groups tabs.
/// 相邻边缘吸附保留独立面板，拖到标题栏才合并为标签组。
fn adjacent_rect(source:Rect,other:Rect)->Option<Rect>{
    let overlap_x=source.right().min(other.right())-source.left().max(other.left());
    let overlap_y=source.bottom().min(other.bottom())-source.top().max(other.top());
    let mut choices=Vec::with_capacity(4);
    if overlap_y>32. {
        choices.push(((source.left()-other.right()-GAP).abs(),egui::pos2(other.right()+GAP,source.top())));
        choices.push(((source.right()-other.left()+GAP).abs(),egui::pos2(other.left()-GAP-source.width(),source.top())));
    }
    if overlap_x>32. {
        choices.push(((source.top()-other.bottom()-GAP).abs(),egui::pos2(source.left(),other.bottom()+GAP)));
        choices.push(((source.bottom()-other.top()+GAP).abs(),egui::pos2(source.left(),other.top()-GAP-source.height())));
    }
    let (_,mut position)=choices.into_iter().filter(|(distance,_)|*distance<=SNAP).min_by(|a,b|a.0.total_cmp(&b.0))?;
    if (source.top()-other.top()).abs()<SNAP{position.y=other.top();}
    if (source.left()-other.left()).abs()<SNAP{position.x=other.left();}
    Some(Rect::from_min_size(position,source.size()))
}
enum Action {Float(Page,Option<Pos2>,bool),Dock(u64),DockPage(Page),Fold(Page),Close(Page),Activate(Page)}

pub(crate) struct Dock {
    panels:BTreeMap<Page,Panel>,order:Vec<Page>,groups:Vec<Group>,next:u64,drag:Option<Drag>,press:Option<Press>,focus:Option<Page>,operation:String,initialized:bool,
    tool_heights:BTreeMap<String,f32>,
}
impl Default for Dock {fn default()->Self{Self{panels:BTreeMap::new(),order:Page::ALL.to_vec(),groups:vec![],next:1,drag:None,press:None,focus:None,operation:String::new(),initialized:false,tool_heights:BTreeMap::new()}}}
pub(crate) struct Output {pub width:f32,pub collapsed:bool,pub closed:Vec<Page>}

impl Dock {
    pub fn restore(value:&serde_json::Value)->Self {
        let mut dock=Self::default();if value["schema"]!=1{return dock;}
        dock.operation=value["operation"].as_str().unwrap_or("").chars().take(64).collect();
        if let Some(heights)=value["tool_heights"].as_object(){for (operation,height) in heights.iter().take(64){
            if !operation.is_empty() && operation.len()<=64{if let Some(height)=height.as_f64().filter(|height|height.is_finite()){
                dock.tool_heights.insert(operation.clone(),height.clamp(96.,1200.) as f32);
            }}
        }}
        if let Some(order)=value["order"].as_array(){let order=order.iter().filter_map(|v|v.as_str().and_then(Page::from_id)).collect::<Vec<_>>();if order.len()==Page::ALL.len() && order.iter().copied().collect::<BTreeSet<_>>().len()==Page::ALL.len(){dock.order=order;}}
        if let Some(panels)=value["panels"].as_array(){for panel in panels.iter().take(Page::ALL.len()) {let Some(page)=panel["page"].as_str().and_then(Page::from_id)else{continue;};let state=dock.panels.entry(page).or_default();state.open=panel["open"].as_bool().unwrap_or(false);state.folded=panel["folded"].as_bool().unwrap_or(false);state.height=panel["height"].as_f64().filter(|v|v.is_finite()).unwrap_or(300.).clamp(96.,1200.) as f32;state.last_rect=saved_rect(&panel["last_rect"]);state.last_size=saved_size(&panel["last_size"]);}}
        let mut grouped=BTreeSet::new();
        if let Some(groups)=value["floating_groups"].as_array(){for group in groups.iter().take(Page::ALL.len()) {
            let Some(values)=group["rect"].as_array().filter(|v|v.len()==4)else{continue;};let coordinates=values.iter().filter_map(|v|v.as_f64().filter(|n|n.is_finite() && n.abs()<1e6).map(|n|n as f32)).collect::<Vec<_>>();if coordinates.len()!=4 || coordinates[2]-coordinates[0]<180. || coordinates[3]-coordinates[1]<24.{continue;}
            let pages=group["pages"].as_array().into_iter().flatten().filter_map(|v|v.as_str().and_then(Page::from_id)).filter(|page|dock.panels.contains_key(page) && grouped.insert(*page)).collect::<Vec<_>>();if pages.is_empty(){continue;}
            let active=group["active"].as_str().and_then(Page::from_id).filter(|p|pages.contains(p)).unwrap_or(pages[0]);let id=dock.next;dock.next+=1;
            for page in &pages{dock.panels.entry(*page).or_default().floating=Some(id);}
            let rect=Rect::from_min_max(egui::pos2(coordinates[0],coordinates[1]),egui::pos2(coordinates[2],coordinates[3]));
            let size=group["size"].as_array().filter(|v|v.len()==2).and_then(|v|Some(egui::vec2(v[0].as_f64()? as f32,v[1].as_f64()? as f32))).filter(|v|v.is_finite()).unwrap_or(rect.size());
            let size=egui::vec2(size.x.clamp(220.,2400.),size.y.clamp(96.,2400.));
            dock.groups.push(Group{id,pages,active,rect,size,position:Some(rect.min),folded:group["folded"].as_bool().unwrap_or(false),fresh:true});
        }}dock
    }
    pub fn was_open(&self,page:Page)->bool{self.panels.get(&page).is_some_and(|state|state.open)}
    pub fn operation(&self)->&str{&self.operation}
    pub fn remember_operation(&mut self,operation:String){self.operation=operation;}
    fn sync(&mut self,requested:&[Page],operation:&str)->bool {
        let first=!self.initialized;self.initialized=true;
        let mut reveal=false;
        for page in Page::ALL {
            let open=requested.contains(&page);let state=self.panels.entry(page).or_default();
            if open && !state.open {self.focus=Some(page);reveal=true;}
            state.open=open;
        }
        for group in &mut self.groups {group.pages.retain(|page|self.panels.get(page).is_some_and(|state|state.floating==Some(group.id)));if self.focus.is_some_and(|page|group.pages.contains(&page)){group.active=self.focus.unwrap();if group.folded{group.folded=false;group.fresh=true;group.position=Some(group.rect.min);}}if !group.pages.contains(&group.active){if let Some(page)=group.pages.first(){group.active=*page;}}}
        self.groups.retain(|group|!group.pages.is_empty());
        if self.operation!=operation {
            if !self.operation.is_empty(){if let Some(state)=self.panels.get(&Page::Tool){self.tool_heights.insert(self.operation.clone(),state.height);}}
            self.operation=operation.into();self.focus=Some(Page::Tool);
            let state=self.panels.entry(Page::Tool).or_default();state.folded=false;
            state.height=self.tool_heights.get(operation).copied().unwrap_or(default_tool_height(operation));reveal=true;
        }else if first && default_tool_height(operation)==600. && !self.tool_heights.contains_key(operation){
            // Migrate the old 300px default once; keep explicitly resized layouts.
            // 旧默认高度仅迁移一次；用户主动调整过的高度仍按原状态恢复。
            if let Some(state)=self.panels.get_mut(&Page::Tool){if (state.height-300.).abs()<0.01{state.height=600.;}}
        }
        reveal && !first
    }
    fn float(&mut self,page:Page,position:Option<Pos2>,manual:bool) {
        let origin=self.panels.get(&page).and_then(|state|state.floating).and_then(|id|self.groups.iter().find(|group|group.id==id)).map(|group|Origin::Floating(group.rect)).unwrap_or_else(||Origin::Dock(self.order.iter().position(|candidate|*candidate==page).unwrap_or(0)));
        for group in &mut self.groups {group.pages.retain(|candidate|*candidate!=page);if !group.pages.contains(&group.active){if let Some(first)=group.pages.first(){group.active=*first;}}}
        self.groups.retain(|group|!group.pages.is_empty());
        // Reuse at most ten native Window IDs per context, avoiding unbounded egui cache keys.
        let id=(1..=Page::ALL.len() as u64).find(|id|!self.groups.iter().any(|group|group.id==*id)).expect("one group per page at most");
        let state=self.panels.entry(page).or_default();state.floating=Some(id);
        let size=state.last_size.unwrap_or(egui::vec2(420.,420.));let position=position.or(state.last_rect.map(|rect|rect.min)).unwrap_or(egui::pos2(80.,120.));
        self.groups.push(Group{id,pages:vec![page],active:page,rect:Rect::from_min_size(position,size),size,position:Some(position),folded:false,fresh:true});
        if manual{self.drag=Some(Drag{group:id,manual:Some(egui::vec2(40.,12.)),origin});}
    }
    fn attach(&mut self,id:u64,target:Target) {
        if let Target::Align(rect)=target{if let Some(group)=self.groups.iter_mut().find(|group|group.id==id){group.rect=rect;group.position=Some(rect.min);}return;}
        let Some(index)=self.groups.iter().position(|group|group.id==id)else{return;};
        if matches!(target,Target::Group(other) if other==id){return;}
        if let Target::Group(other)=target {if !self.groups.iter().any(|group|group.id==other){return;}}
        let source=self.groups.remove(index);
        match target {
            Target::Dock(index)=>{
                let removed=self.order.iter().take(index).filter(|page|source.pages.contains(page)).count();
                self.order.retain(|page|!source.pages.contains(page));let insertion=index.saturating_sub(removed).min(self.order.len());
                for (offset,page) in source.pages.iter().enumerate(){self.panels.entry(*page).or_default().floating=None;self.order.insert(insertion+offset,*page);}
                self.focus=Some(source.active);
            },
            Target::Group(other)=>{let group=self.groups.iter_mut().find(|group|group.id==other).unwrap();for page in source.pages {self.panels.entry(page).or_default().floating=Some(other);group.pages.push(page);}group.active=source.active;},
            Target::Align(_)=>unreachable!(),
        }
    }
    pub fn summary(&self)->serde_json::Value {
        let mut heights=self.tool_heights.clone();if !self.operation.is_empty(){if let Some(state)=self.panels.get(&Page::Tool){heights.insert(self.operation.clone(),state.height);}}
        serde_json::json!({"schema":1,"operation":self.operation,"tool_heights":heights,"order":self.order.iter().map(|page|format!("{page:?}")).collect::<Vec<_>>(),"panels":self.order.iter().filter_map(|page|self.panels.get(page).map(|state|serde_json::json!({"page":format!("{page:?}"),"open":state.open,"floating_group":state.floating,"folded":state.folded,"height":state.height,"last_rect":state.last_rect.map(|r|[r.min.x,r.min.y,r.max.x,r.max.y]),"last_size":state.last_size.map(|size|[size.x,size.y])}))).collect::<Vec<_>>(),"floating_groups":self.groups.iter().map(|group|serde_json::json!({"id":group.id,"pages":group.pages.iter().map(|page|format!("{page:?}")).collect::<Vec<_>>(),"active":format!("{:?}",group.active),"rect":[group.rect.min.x,group.rect.min.y,group.rect.max.x,group.rect.max.y],"folded":group.folded,"size":[group.size.x,group.size.y]})).collect::<Vec<_>>()})
    }
    pub fn show(&mut self,ctx:&egui::Context,spectrum:bool,requested:&[Page],operation:&str,width:f32,mut collapsed:bool,mut body:impl FnMut(&mut egui::Ui,Page))->Output {
        if ctx.input(|input|input.key_pressed(egui::Key::Escape)){
            if let Some(drag)=self.drag.take(){match drag.origin{Origin::Dock(index)=>self.attach(drag.group,Target::Dock(index)),Origin::Floating(rect)=>{if let Some(group)=self.groups.iter_mut().find(|group|group.id==drag.group){group.rect=rect;group.position=Some(rect.min);}}}}
            self.press=None;ctx.stop_dragging();
        }
        let fresh=!self.initialized;if self.sync(requested,operation){collapsed=false;}
        let mut actions=vec![];let mut closed=vec![];let mut reset=false;let mut pane_rects=vec![];let mut drawn=BTreeSet::new();
        let available=ctx.available_rect().width().max(1.);let maximum=(available-(available*0.5).min(320.)).clamp(1.,1200.);let minimum=220_f32.min(maximum);
        let mut side=egui::SidePanel::right(if spectrum{"spectrum_pro_tools"}else{"image_pro_tools"}).frame(egui::Frame::NONE.inner_margin(6.).fill(ctx.style().visuals.panel_fill));
        side=if collapsed{side.resizable(false).exact_width(44.)}else if fresh{side.resizable(true).exact_width(width.clamp(minimum,maximum))}else{side.resizable(true).default_width(width.clamp(minimum,maximum)).width_range(minimum..=maximum)};
        let panel=side.show(ctx,|ui|{
            crate::ribbon::align_controls(ui);ui.spacing_mut().item_spacing=egui::vec2(4.,GAP);
            if collapsed {
                if icon_button(ui,Icon::Expand,t("Expand work area")).clicked(){collapsed=false;}
                ui.strong("PRO");
                for page in self.order.iter().copied().filter(|page|requested.contains(page)) {
                    if ui.add_sized([CONTROL,CONTROL],egui::Button::new(page.short())).on_hover_text(t(page.title())).clicked(){actions.push(Action::Activate(page));collapsed=false;}
                }
                return;
            }
            egui::Frame::group(ui.style()).inner_margin(6.).show(ui,|ui|{
                ui.set_min_height(ui.available_height());
                let size=CONTROL.max(ui.spacing().interact_size.y);let gap=ui.spacing().item_spacing.x;
                let (row,_)=centered_heading(ui,t(if spectrum{"Spectrum PRO workspace"}else{"PRO workspace"}),2.*(size+gap),Sense::hover());
                let mut actions_ui=ui.new_child(egui::UiBuilder::new().id_salt("workspace-actions").max_rect(row).layout(egui::Layout::right_to_left(egui::Align::Center)));
                if icon_button(&mut actions_ui,Icon::Fold,t("Collapse work area to rail")).clicked(){collapsed=true;}
                information(&mut actions_ui,"workspace-help",|ui|{ui.label(t("Drag a panel title to float it. Release near another panel to dock or group it."));if ui.button(t("Reset workspace layout")).clicked(){reset=true;ui.close();}});
                ui.separator();
                egui::ScrollArea::vertical().id_salt("pro-pane-stack").auto_shrink([false,false]).show(ui,|ui|{
                    for page in self.order.clone() {
                        let state=self.panels.entry(page).or_default();if !state.open || state.floating.is_some(){continue;}
                        let folded=state.folded;let height=state.height;let mut next_height=height;
                        let frame=ui.push_id(page,|ui|egui::Frame::group(ui.style()).inner_margin(6.).show(ui,|ui|{
                            ui.set_width(ui.available_width());
                            let size=CONTROL.max(ui.spacing().interact_size.y);let gap=ui.spacing().item_spacing.x;
                            let side_inset=if page==Page::Tool{size+gap}else{2.*(size+gap)};
                            let (row,response)=centered_heading(ui,t(page.title()),side_inset,Sense::click_and_drag());
                            if response.on_hover_text(t("Drag to float panel")).dragged(){actions.push(Action::Float(page,ctx.pointer_interact_pos().map(|p|p-egui::vec2(40.,12.)),true));}
                            let mut leading_ui=ui.new_child(egui::UiBuilder::new().id_salt("panel-fold").max_rect(row).layout(egui::Layout::left_to_right(egui::Align::Center)));
                            if icon_button(&mut leading_ui,if folded{Icon::Expand}else{Icon::Fold},t("Collapse / expand panel")).clicked(){actions.push(Action::Fold(page));}
                            let mut trailing_ui=ui.new_child(egui::UiBuilder::new().id_salt("panel-actions").max_rect(row).layout(egui::Layout::right_to_left(egui::Align::Center)));
                            if page!=Page::Tool && icon_button(&mut trailing_ui,Icon::Close,t("Close panel")).clicked(){actions.push(Action::Close(page));}
                            if icon_button(&mut trailing_ui,Icon::Float,t("Float panel")).clicked(){actions.push(Action::Float(page,None,false));}
                            if !folded {
                                ui.separator();egui::ScrollArea::vertical().id_salt("panel-content").max_height(height).auto_shrink([false,false]).show(ui,|ui|{if drawn.insert(page){body(ui,page);}});
                                let (rect,response)=ui.allocate_exact_size(egui::vec2(ui.available_width(),6.),Sense::drag());
                                ui.painter().line_segment([rect.center()-egui::vec2(18.,0.),rect.center()+egui::vec2(18.,0.)],ui.visuals().widgets.noninteractive.bg_stroke);
                                response.clone().on_hover_cursor(egui::CursorIcon::ResizeVertical);
                                if response.dragged(){next_height=(height+response.drag_delta().y).clamp(96.,1200.);}
                            }
                        })).inner.response.rect;
                        self.panels.entry(page).or_default().height=next_height;
                        if self.focus==Some(page){ui.scroll_to_rect(frame,Some(egui::Align::Min));self.focus=None;}
                        pane_rects.push((page,frame));ui.add_space(2.);
                    }
                });
            });
        });
        // UI events are applied after drawing to avoid duplicate bodies or mutable aliases.
        for action in actions.drain(..) {match action {
            Action::Float(page,position,manual)=>self.float(page,position,manual),
            Action::Fold(page)=>{let state=self.panels.entry(page).or_default();state.folded=!state.folded;},
            Action::Close(page)=>{self.panels.entry(page).or_default().open=false;closed.push(page);},
            Action::Activate(page)=>{let state=self.panels.entry(page).or_default();state.folded=false;self.focus=Some(page);if let Some(id)=state.floating{if let Some(group)=self.groups.iter_mut().find(|group|group.id==id){group.active=page;group.folded=false;group.fresh=true;group.position=Some(group.rect.min);}}},
            Action::Dock(_)|Action::DockPage(_)=>{},
        }}
        for group in &mut self.groups {
            let visible=group.pages.iter().copied().filter(|page|self.panels.get(page).is_some_and(|state|state.open)).collect::<Vec<_>>();
            if visible.is_empty(){continue;}
            if !visible.contains(&group.active){group.active=visible[0];}
            let mut open=true;
            if let Some(Drag{group:id,manual:Some(offset),..})=self.drag {if id==group.id && ctx.input(|i|i.pointer.primary_down()){group.position=ctx.pointer_interact_pos().map(|point|point-offset);}}
            let id=egui::Id::new(("pro-floating-group",spectrum,group.id));
            if group.fresh{let mut state=egui::collapsing_header::CollapsingState::load_with_default_open(ctx,id.with("collapsing"),!group.folded);state.set_open(!group.folded);state.store(ctx);}
            let screen=ctx.screen_rect().shrink(8.);let maximum=(screen.size()-egui::vec2(16.,40.)).max(egui::vec2(1.,1.));group.size=group.size.min(maximum);
            let caption=crate::ui_style::window_caption(ctx,&format!("PRO · {}",t(group.active.title())));
            let padded=|size|crate::ui_style::padded_window_size(ctx,size);
            let mut window=egui::Window::new(egui::WidgetText::Galley(caption.clone())).id(id).open(&mut open).resizable(true).collapsible(true).constrain_to(screen)
                .frame(crate::ui_style::compact_window_frame(ctx)).min_size(padded(maximum.min(egui::vec2(220.,64.)))).max_size(padded(maximum)).default_open(!group.folded).default_size(padded(group.size));
            if group.fresh{window=window.fixed_size(padded(group.size));group.fresh=false;}
            if let Some(position)=group.position.take(){let screen=ctx.screen_rect();let position=egui::pos2(position.x.clamp(screen.left(),(screen.right()-96.).max(screen.left())),position.y.clamp(screen.top(),(screen.bottom()-32.).max(screen.top())));window=window.current_pos(position);}
            if let Some(response)=window.show(ctx,|ui|crate::ui_style::window_body(ui,|ui|{
                crate::ribbon::align_controls(ui);
                ui.horizontal_wrapped(|ui|{
                    for page in &visible {if ui.selectable_label(group.active==*page,t(page.title())).clicked(){group.active=*page;}}
                    if icon_button(ui,Icon::Dock,t("Dock panel group")).clicked(){actions.push(Action::Dock(group.id));}
                    if visible.len()>1 && icon_button(ui,Icon::Float,t("Float selected tab")).clicked(){actions.push(Action::Float(group.active,Some(group.rect.min+egui::vec2(24.,24.)),false));}
                    if group.active!=Page::Tool && icon_button(ui,Icon::Close,t("Close panel")).clicked(){actions.push(Action::Close(group.active));}
                });ui.separator();egui::ScrollArea::vertical().id_salt(("floating-content",group.id)).auto_shrink([false,false]).show(ui,|ui|{if drawn.insert(group.active){body(ui,group.active);}else{ctx.request_repaint();}});
                ui.min_rect().size()
            })){
                crate::ui_style::weight_window_title(ctx,&response.response,&caption);
                let rect=response.response.rect;
                if ctx.input(|i|i.pointer.primary_pressed()) {if let Some(origin)=ctx.pointer_interact_pos(){
                    let title=crate::ui_style::window_title_rect(ctx,rect).shrink2(egui::vec2(32.,0.));
                    if title.contains(origin) && ctx.layer_id_at(origin)==Some(response.response.layer_id){self.press=Some(Press{group:group.id,origin,rect});}
                }}
                if let (Some(press),Some(pointer))=(self.press,ctx.pointer_interact_pos()) {if press.group==group.id && ctx.input(|i|i.pointer.primary_down()) && pointer.distance(press.origin)>6. && rect.min.distance(press.rect.min)>2. && (rect.size()-press.rect.size()).length()<1.{self.drag=Some(Drag{group:group.id,manual:None,origin:Origin::Floating(press.rect)});}}
                group.rect=rect;group.folded=response.inner.is_none();if let Some(size)=response.inner{group.size=size;}
                for page in &group.pages{let panel=self.panels.entry(*page).or_default();panel.last_rect=Some(rect);panel.last_size=Some(group.size);}
            }
            if !open {for page in &visible {if *page==Page::Tool{actions.push(Action::DockPage(*page));}else{actions.push(Action::Close(*page));}}}
        }
        let pointer=ctx.pointer_interact_pos();let mut target=None;
        if let (Some(drag),Some(pointer))=(self.drag,pointer) {
            if panel.response.rect.expand(SNAP).contains(pointer) {
                let insertion=pane_rects.iter().find(|(_,rect)|pointer.y<rect.center().y).and_then(|(page,_)|self.order.iter().position(|candidate|candidate==page)).unwrap_or(self.order.len());
                target=Some((Target::Dock(insertion),panel.response.rect.shrink(3.)));
            }else {
                let others=self.groups.iter().filter(|group|group.id!=drag.group && group.pages.iter().any(|page|self.panels.get(page).is_some_and(|state|state.open))).collect::<Vec<_>>();
                if let Some(group)=others.iter().find(|group|crate::ui_style::window_title_rect(ctx,group.rect).contains(pointer)){target=Some((Target::Group(group.id),group.rect));}
                else if let Some(source)=self.groups.iter().find(|group|group.id==drag.group){
                    if let Some(rect)=others.iter().filter_map(|group|adjacent_rect(source.rect,group.rect)).min_by(|a,b|a.min.distance_sq(source.rect.min).total_cmp(&b.min.distance_sq(source.rect.min))){target=Some((Target::Align(rect),rect));}
                }
            }
            if let Some((_,rect))=target {
                let painter=ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground,egui::Id::new("pro-dock-preview")));
                painter.rect_stroke(rect,4.,Stroke::new(2_f32,ctx.style().visuals.selection.bg_fill),egui::StrokeKind::Inside);
                if !matches!(target,Some((Target::Align(_),_))){painter.text(rect.center_top()+egui::vec2(0.,8.),egui::Align2::CENTER_TOP,t("Release to dock"),egui::FontId::proportional(13.),ctx.style().visuals.strong_text_color());}
            }
            if ctx.input(|i|i.pointer.primary_released()){if let Some((target,_))=target {self.attach(drag.group,target);if matches!(target,Target::Dock(_)){collapsed=false;}}self.drag=None;}
        }
        for action in actions {match action {Action::Dock(id)=>{self.attach(id,Target::Dock(0));collapsed=false;},Action::DockPage(page)=>{for group in &mut self.groups{group.pages.retain(|candidate|*candidate!=page);}self.groups.retain(|group|!group.pages.is_empty());self.panels.entry(page).or_default().floating=None;self.focus=Some(page);collapsed=false;},Action::Float(page,position,manual)=>self.float(page,position,manual),Action::Close(page)=>{self.panels.entry(page).or_default().open=false;closed.push(page);},_=>{}}}
        if !ctx.input(|i|i.pointer.primary_down() || i.pointer.primary_released()){self.drag=None;self.press=None;}
        if ctx.input(|i|i.key_pressed(egui::Key::Escape)){self.drag=None;self.press=None;}
        if reset {self.tool_heights.clear();for (page,state) in &mut self.panels{state.floating=None;state.folded=false;state.height=if *page==Page::Tool{default_tool_height(&self.operation)}else{300.};}self.groups.clear();self.order=Page::ALL.to_vec();self.drag=None;collapsed=false;}
        Output{width:panel.response.rect.width(),collapsed,closed}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]fn adjacent_edges_snap_without_merging_windows(){
        let other=Rect::from_min_size(egui::pos2(100.,100.),egui::vec2(300.,400.));
        let source=Rect::from_min_size(egui::pos2(415.,110.),egui::vec2(240.,300.));
        let aligned=adjacent_rect(source,other).unwrap();assert_eq!(aligned.left(),406.);assert_eq!(aligned.top(),100.);
        assert!(adjacent_rect(source.translate(egui::vec2(100.,0.)),other).is_none());
        let mut dock=Dock::default();dock.sync(&[Page::Tool,Page::Coordinates],"wcs");dock.float(Page::Coordinates,None,false);let id=dock.groups[0].id;
        dock.attach(id,Target::Align(aligned));assert_eq!(dock.groups.len(),1);assert_eq!(dock.groups[0].position,Some(aligned.min));assert_eq!(dock.panels[&Page::Coordinates].floating,Some(id));
    }
    #[test]fn floating_pages_merge_and_return_without_duplicate_owners(){
        let mut dock=Dock::default();dock.sync(&[Page::Tool,Page::Coordinates,Page::Photometry],"detect_stars");
        dock.float(Page::Coordinates,None,false);let first=dock.groups[0].id;dock.float(Page::Photometry,None,false);let second=dock.groups[1].id;
        dock.attach(second,Target::Group(first));assert_eq!(dock.groups.len(),1);assert_eq!(dock.groups[0].pages,vec![Page::Coordinates,Page::Photometry]);
        dock.attach(first,Target::Dock(0));assert!(dock.groups.is_empty());assert_eq!(dock.order.iter().filter(|p|**p==Page::Coordinates).count(),1);assert!(dock.panels[&Page::Photometry].floating.is_none());
    }
    #[test]fn closing_and_reopening_a_page_preserves_its_floating_placement(){
        let mut dock=Dock::default();dock.sync(&[Page::Tool,Page::Coordinates],"detect_stars");dock.float(Page::Coordinates,None,false);
        dock.sync(&[Page::Tool],"detect_stars");assert_eq!(dock.groups.len(),1);dock.sync(&[Page::Tool,Page::Coordinates],"detect_stars");assert_eq!(dock.panels[&Page::Coordinates].floating,Some(dock.groups[0].id));
    }
    #[test]fn restored_layout_keeps_geometry_group_tab_fold_and_closed_pages(){
        let mut dock=Dock::default();dock.sync(&[Page::Tool,Page::Coordinates,Page::Photometry],"detect_stars:");dock.float(Page::Coordinates,Some(egui::pos2(123.,87.)),false);let first=dock.groups[0].id;dock.float(Page::Photometry,None,false);let second=dock.groups[1].id;dock.attach(second,Target::Group(first));dock.groups[0].folded=true;dock.panels.get_mut(&Page::Tool).unwrap().folded=true;
        let restored=Dock::restore(&dock.summary());assert_eq!(restored.groups.len(),1);assert_eq!(restored.groups[0].active,Page::Photometry);assert_eq!(restored.groups[0].rect.min,egui::pos2(123.,87.));assert!(restored.groups[0].folded);assert!(restored.panels[&Page::Tool].folded);assert!(!restored.was_open(Page::Header));
    }
    #[test]fn archive_height_is_larger_and_each_tool_remembers_user_resizing(){
        let mut dock=Dock::default();dock.sync(&[Page::Tool],"detect_stars:");
        dock.panels.get_mut(&Page::Tool).unwrap().height=340.;
        dock.sync(&[Page::Tool],"archive_search:");assert_eq!(dock.panels[&Page::Tool].height,600.);
        dock.panels.get_mut(&Page::Tool).unwrap().height=480.;
        dock.sync(&[Page::Tool],"detect_stars:");assert_eq!(dock.panels[&Page::Tool].height,340.);
        let mut restored=Dock::restore(&dock.summary());restored.sync(&[Page::Tool],"archive_search:");
        assert_eq!(restored.panels[&Page::Tool].height,480.);
        restored.sync(&[Page::Tool],"detect_stars:");assert_eq!(restored.panels[&Page::Tool].height,340.);
        let legacy=serde_json::json!({"schema":1,"operation":"archive_search:","panels":[{"page":"Tool","open":true,"height":300.}]});
        let mut migrated=Dock::restore(&legacy);migrated.sync(&[Page::Tool],"archive_search:");
        assert_eq!(migrated.panels[&Page::Tool].height,600.);
        migrated.panels.get_mut(&Page::Tool).unwrap().height=300.;
        let mut explicit=Dock::restore(&migrated.summary());explicit.sync(&[Page::Tool],"archive_search:");
        assert_eq!(explicit.panels[&Page::Tool].height,300.);
        // Bare legacy keys and full task keys share defaults, never saved sizes.
        // 无后缀的历史键与完整子任务键共用默认值，但不混用已保存尺寸。
        assert_eq!(default_tool_height("archive_search"),600.);
        explicit.sync(&[Page::Tool],"archive_search:manual");
        assert_eq!(explicit.panels[&Page::Tool].height,600.);
        explicit.panels.get_mut(&Page::Tool).unwrap().height=520.;
        explicit.sync(&[Page::Tool],"archive_search:");
        assert_eq!(explicit.panels[&Page::Tool].height,300.);
        explicit.sync(&[Page::Tool],"archive_search:manual");
        assert_eq!(explicit.panels[&Page::Tool].height,520.);
    }
}



