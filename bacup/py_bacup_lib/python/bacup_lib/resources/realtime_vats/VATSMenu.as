package {
    import Shared.AS3.IMenu;

    public class VATSMenu extends IMenu {
        public var B21BridgeVersion:uint;

        public function B21Initialize(provider:String, callback:Function):void {
            this.B21BridgeVersion = 2;
            this.m_ScreenRatio = this.STAGE_RATIO;
            this.bShowButtonHelp = true;
            this.bShowPlaybackButtons = false;
            this.bCriticalsEnabled = false;
            this.DisableCriticalButton();
            this.UpdateButtonVisibility();
        }

        public function B21SetFrame(frame:Object):void {
            var parts:Array = frame.parts as Array;
            if (parts == null) parts = [];
            while (this.PartInfos.length > parts.length) this.removeChild(this.PartInfos.pop());
            while (this.PartInfos.length < parts.length) {
                var created:PartInfo = new PartInfo();
                this.addChild(created);
                this.PartInfos.push(created);
            }
            var positions:Array = [];
            for (var i:uint = 0; i < parts.length; i++) {
                var part:PartInfo = this.PartInfos[i] as PartInfo;
                part.SetName(String(parts[i].name));
                part.SetChanceToHit(uint(Math.max(0, Math.min(100, Number(parts[i].chance)))));
                part.SetHealthPercent(Math.max(0, Math.min(1, Number(parts[i].health))));
                part.SetSelected(i == uint(frame.selected));
                part.SetActionCount(0);
                positions.push({x:parts[i].x, y:parts[i].y, visible:parts[i].visible});
            }
            this.SelectedPart = uint(frame.selected);
            this.m_ScreenRatio = this.STAGE_RATIO;
            this.UpdatePartPositions(positions);
            this.bCriticalsEnabled = Boolean(frame.criticalReady);
            if (this.bCriticalsEnabled) this.EnableCriticalButton();
            else this.DisableCriticalButton();
            this.bShowPlaybackButtons = false;
            this.UpdateButtonVisibility();
        }

        public function B21Clear():void {
            this.B21SetFrame({parts:[], selected:uint.MAX_VALUE, criticalReady:false});
        }
    }
}
