package
{
   import flash.display.MovieClip;
   import flash.display.Loader;
   import flash.display.DisplayObjectContainer;
   import flash.events.Event;
   import flash.events.IOErrorEvent;
   import flash.events.MouseEvent;
   import flash.net.URLRequest;
   import flash.system.ApplicationDomain;
   import flash.system.LoaderContext;
   import flash.text.TextField;
   import flash.text.TextFormat;
   import Shared.AS3.BCGridList;
   import Shared.AS3.BSButtonHintBar;
   import Shared.AS3.SWFLoaderClip;
   import Shared.ProgressionNode;

   public class PetProgressionTrackMenu extends MovieClip
   {
      public var ButtonHintBar_mc:BSButtonHintBar;
      public var DetailsPanel_mc:MovieClip;
      public var LevelBar_mc:MovieClip;
      public var SelectionBar_mc:MovieClip;
      public var InvalidDataPanel_mc:MovieClip;
      public var Background_mc:MovieClip;
      public var Fader_mc:MovieClip;
      public var PageTransition_mc:MovieClip;
      public var ItemIcon_mc:SWFLoaderClip;
      public var ProgressionGrid_mc:BCGridList;
      public var BGSCodeObj:Object;
      public var PetProgressionVersion:uint = 2;
      public var BoardReady:Boolean = false;
      private var tracks:Array = [];
      private var page:int = 0;
      private var names:Array = ["Dog", "Cat", "Radhog", "Deathclaw"];
      private var claimDisabled:Boolean = true;
      private var hint:TextField = new TextField();
      private var icons:Loader = new Loader();
      private var iconsReady:Boolean = false;
      private var detailIcon:MovieClip;
      private var pageIcons:Array = [null, null, null];
      private var speciesIcons:Array = ["WpIconDogSmall", "WpIconCatSmall", "WpIconRadhogSmall", "WpIconDCSmall"];

      public function PetProgressionTrackMenu()
      {
         super();
         stop(); focusRect = false;
         addEventListener(Event.ADDED_TO_STAGE, ready);
      }
      private function ready(event:Event):void
      {
         if (stage) stage.stageFocusRect = false;
         labels(this);
         if (ButtonHintBar_mc != null) ButtonHintBar_mc.visible = false;
         Fader_mc.visible = false;
         PageTransition_mc.visible = false;
         InvalidDataPanel_mc.visible = false;
         ProgressionGrid_mc.maxCols = 8;
         ProgressionGrid_mc.maxRows = 4;
         ProgressionGrid_mc.scrollVertical = true;
         ProgressionGrid_mc.textBehavior = BCGridList.TEXT_OPTION_NONE;
         ProgressionGrid_mc.useVariableWidth = false;
         ProgressionGrid_mc.useVariableHeight = false;
         ProgressionGrid_mc.listItemClassName = "Shared.ProgressionNode";
         ProgressionGrid_mc.addEventListener(BCGridList.SELECTION_CHANGE, selection);
         ProgressionGrid_mc.addEventListener(BCGridList.ITEM_CLICKED, accept);
         SelectionBar_mc.PreviousArrow_mc.addEventListener(MouseEvent.CLICK, previous);
         SelectionBar_mc.NextArrow_mc.addEventListener(MouseEvent.CLICK, next);
         DetailsPanel_mc.Scrollup_btn.addEventListener(MouseEvent.CLICK, scrollUp);
         DetailsPanel_mc.Scrolldown_btn.addEventListener(MouseEvent.CLICK, scrollDown);
         hint.defaultTextFormat = new TextFormat("$MAIN_Font", 24, 0x31210F);
         hint.x = 210; hint.y = 1030; hint.width = 1500; hint.height = 40;
         hint.selectable = false;
         hint.text = "[Z / LB] Previous    [C / RB] Next    [Enter / A] Claim    [Tab / B] Exit";
         addChild(hint);
         DetailsPanel_mc.Claim_mc.addEventListener(MouseEvent.CLICK, accept);
         icons.contentLoaderInfo.addEventListener(Event.COMPLETE, loaded);
         icons.contentLoaderInfo.addEventListener(IOErrorEvent.IO_ERROR, failed);
         icons.load(new URLRequest("ProgressionIconLibrary.swf"), new LoaderContext(false, ApplicationDomain.currentDomain));
      }
      private function loaded(event:Event):void { iconsReady = true; if (tracks.length) draw(); }
      private function failed(event:IOErrorEvent):void { dispatch("unavailable"); }
      private function labels(container:DisplayObjectContainer):void
      {
         for (var i:int = 0; i < container.numChildren; ++i) {
            var child:Object = container.getChildAt(i);
            if (child is TextField && (child.text == "$Pet Rewards" || child.text == "$PET PROGRESSION" || child.text == "$Level"))
               child.text = child.text.substr(1);
            else if (child is DisplayObjectContainer) labels(DisplayObjectContainer(child));
         }
      }
      private function pageIcon(slot:int, holder:Object, track:Object):void
      {
         if (pageIcons[slot] != null && pageIcons[slot].parent == holder) holder.removeChild(pageIcons[slot]);
         pageIcons[slot] = holder.setContainerIconClip(speciesIcons[uint(track.species)]);
      }
      public function B21SetData(data:Object):void
      {
         tracks = data.tracks as Array;
         if (tracks == null || !tracks.length) { close(); return; }
         page = Math.max(0, Math.min(tracks.length - 1, int(data.page)));
         if (iconsReady) draw();
      }
      private function draw():void
      {
         var track:Object = tracks[page];
         var entries:Array = [];
         for each (var row:Object in track.nodes)
         {
            var node:ProgressionNode = new ProgressionNode();
            node.LevelThreshhold = int(row.level);
            node.Name = String(row.name);
            node.DescriptionTitle = String(row.title);
            node.DescriptionText = String(row.description);
            node.IconKeyword = String(row.icon);
            node.UID = String(row.uid);
            node.Earned = int(row.level) <= int(track.level);
            node.Claimed = Boolean(row.claimed);
            node.Enabled = true;
            node.Special = int(row.level) % 50 == 0;
            node.Empty = false;
            entries.push(node);
         }
         ProgressionGrid_mc.entryData = entries;
         ProgressionGrid_mc.selectedIndex = 0;
         ProgressionGrid_mc.invalidateData();
         SelectionBar_mc.Name_tf.text = names[uint(track.species)];
         SelectionBar_mc.Level_tf.text = String(track.level);
         pageIcon(0, SelectionBar_mc.CurrentIcon_mc.ItemIcon_mc, track);
         pageIcon(1, SelectionBar_mc.PreviousArrow_mc.LastIcon_mc.ItemIcon_mc, tracks[(page + tracks.length - 1) % tracks.length]);
         pageIcon(2, SelectionBar_mc.NextArrow_mc.NextIcon_mc.ItemIcon_mc, tracks[(page + 1) % tracks.length]);
         LevelBar_mc.gotoAndStop(Math.max(1, Math.min(100, int(100 * Number(track.fraction)))));
         LevelBar_mc.CurrentLevel_mc.CurrentLevel_tf.text = String(track.level);
         LevelBar_mc.NextLevel_mc.NextLevel_tf.text = String(track.nextLevel);
         LevelBar_mc.XPLabel_tf.text = String(track.xpLabel);
         selection(null);
         labels(this);
         BoardReady = true;
      }
      public function B21RefreshData(data:Object):void
      {
         var selected:int = ProgressionGrid_mc.selectedIndex;
         var updated:Array = data.tracks as Array;
         if (updated == null || updated.length != tracks.length) return;
         tracks = updated;
         draw();
         ProgressionGrid_mc.selectedIndex = Math.max(0, Math.min(selected, tracks[page].nodes.length - 1));
         selection(null);
      }
      private function selection(event:Event):void
      {
         var index:int = ProgressionGrid_mc.selectedIndex;
         if (index < 0 || index >= tracks[page].nodes.length) return;
         var row:Object = tracks[page].nodes[index];
         DetailsPanel_mc.gotoAndStop("Regular");
         DetailsPanel_mc.RewardLevel_tf.text = String(row.level);
         DetailsPanel_mc.RewardName_tf.text = String(row.name);
         DetailsPanel_mc.RewardDescriptionTitle_tf.text = String(row.title);
         DetailsPanel_mc.RewardDesc_tf.text = String(row.description);
         DetailsPanel_mc.RewardDesc_tf.scrollV = 1;
         var earned:Boolean = int(row.level) <= int(tracks[page].level);
         claimDisabled = !earned || !row.claimable || row.claimed;
         DetailsPanel_mc.Claim_mc.gotoAndStop(row.claimed ? "claimed" : earned && row.claimable ? "unclaimed" : "locked");
         if (!row.claimed && DetailsPanel_mc.Claim_mc.Claim_tf != null)
            DetailsPanel_mc.Claim_mc.Claim_tf.text = !earned ? "Unlocks at level " + row.level : row.claimable ? "Claim" : "Reward unavailable";
         if (detailIcon != null && detailIcon.parent != null) detailIcon.parent.removeChild(detailIcon);
         detailIcon = DetailsPanel_mc.ItemIcon_mc.setContainerIconClip(String(row.icon));
         if (detailIcon != null) {
            detailIcon.x = -DetailsPanel_mc.ItemIcon_mc.clipWidth / 2;
            detailIcon.y = -DetailsPanel_mc.ItemIcon_mc.clipHeight / 2;
            detailIcon.gotoAndStop("Earned");
         }
      }
      private function dispatch(action:String, uid:String = ""):void
      {
         if (BGSCodeObj != null && BGSCodeObj.PetProgressionAction != null)
            BGSCodeObj.PetProgressionAction(action, tracks.length ? uint(tracks[page].species) : 0, uid);
      }
      private function claimSelected():void
      {
         var index:int = ProgressionGrid_mc.selectedIndex;
         if (!claimDisabled && index >= 0) dispatch("claim", String(tracks[page].nodes[index].uid));
      }
      private function close():void { dispatch("close"); }
      private function previousPage():void { page = (page + tracks.length - 1) % tracks.length; draw(); }
      private function nextPage():void { page = (page + 1) % tracks.length; draw(); }
      private function previous(event:MouseEvent):void { previousPage(); }
      private function next(event:MouseEvent):void { nextPage(); }
      private function accept(event:Event):void { claimSelected(); }
      private function scrollUp(event:MouseEvent):void { DetailsPanel_mc.RewardDesc_tf.scrollV--; }
      private function scrollDown(event:MouseEvent):void { DetailsPanel_mc.RewardDesc_tf.scrollV++; }
      public function Navigate(action:String):void
      {
         if (action == "Cancel") { close(); return; }
         if (!tracks.length || !iconsReady) return;
         if (action == "Previous") previousPage();
         else if (action == "Next") nextPage();
         else if (action == "Accept") claimSelected();
         else {
            var delta:int = action == "Left" ? -1 : action == "Right" ? 1 : action == "Up" ? -8 : action == "Down" ? 8 : 0;
            ProgressionGrid_mc.selectedIndex = Math.max(0, Math.min(tracks[page].nodes.length - 1, ProgressionGrid_mc.selectedIndex + delta));
            selection(null);
         }
      }
   }
}
