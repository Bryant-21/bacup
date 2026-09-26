package SpecialBuilds {
    import flash.events.Event;
    import flash.external.ExternalInterface;
    import Shared.AS3.IMenu;

    public class SpecialBuildsMenu extends IMenu {
        private var PreviewFrames:uint;
        private var PreviewActions:Array;
        private var PreviewData:Object;

        public function PreviewInit():void {
            PreviewFrames = 0;
            PreviewActions = [];
            addEventListener(Event.ENTER_FRAME, PreviewFrame);
            ExternalInterface.addCallback("command", PreviewCommand);
        }

        public function PreviewFrame(event:Event):void {
            if (++PreviewFrames != 10) return;
            BGSCodeObj = {PunchCardEvent: PreviewAction};
            var stats:Array = [];
            for (var i:uint = 0; i < 7; ++i) stats.push({value:4, perkCards:[]});
            PreviewData = {generation:"1", revision:"1", totalPoints:56,
                builds:[{id:1,name:"Adventure",isLocked:false,isCurrentBuild:true,isValid:true,specials:stats},
                    {id:2,name:"Crafting",isLocked:false,isCurrentBuild:false,isValid:true,specials:stats},
                    {id:0,name:"ADD LOADOUT",isLocked:true,isCurrentBuild:false,isValid:true,specials:[]}]};
            var ok:Boolean = B21SetData(PreviewData, false);
            ExternalInterface.call("report", JSON.stringify({ready:ok,error:B21LastError}));
        }

        public function PreviewAction(action:String, payload:Object):void {
            if (payload is String) throw new Error("Native callback requires an object");
            PreviewActions.push({action:action,payload:payload});
        }

        public function PreviewCommand(action:String):String {
            if (action == "Refresh") B21SetData(PreviewData, false);
            else if (action != "Read") B21Input(action);
            var data:Object = this.EditSpecialModal_mc.PreviewState();
            data.actions = PreviewActions;
            data.rename = B21RenamePanel != null;
            data.name = B21Name == null ? "" : B21Name.text;
            data.nameFocused = B21Name != null && stage.focus == B21Name;
            data.focusRect = stage.stageFocusRect;
            data.children = numChildren;
            data.error = B21LastError;
            return JSON.stringify(data);
        }
    }
}
