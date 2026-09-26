package Shared.AS3 {
    import flash.display.MovieClip;
    public class IMenu extends MovieClip {
        public var platform:uint = 99;
        public var QuantityModal_mc:PreviewQuantity = new PreviewQuantity();
        public var ButtonHintBar_mc:PreviewHintBar = new PreviewHintBar();
        public var Header_mc:MovieClip = new MovieClip();
        public function ConvertEventString(action:String):String { return action; }
        public var _NumLevelUpPoints:int = 0;
        public var _PerkCardData:Array = [];
        public var _ViewUnownedCards:Boolean = false;
        public var _InitialShowQueue:Array;
        public var _NumSPECIALPoints:uint;
        public var _AllowEditsToCardsAndDecks:Boolean;
        public var Collection_mc:MovieClip = new MovieClip();
        public var EquipDeckHolder_mc:MovieClip = new MovieClip();
        public var AcceptButton:Object = {};
        public var RankUpButton:Object = {};
        public var LevelUpButton:Object = {};
        public var OpenCardPackButton:Object = {};
        public var ShareButton:Object = {};
        public var LevelBoostPurchaseButton:Object = {};
        public var _SPECIALToLevel:int = -1;
        public var PickSpecial_mc:MovieClip = new MovieClip();
        public var PickAPerk_mc:MovieClip = new MovieClip();
        public var PerkLevelUpOption_mc:MovieClip = new MovieClip();
        public function ProcessUserEvent(action:String, pressed:Boolean):Boolean { return true; }
        public var queryResults:Array = [];
        public function OnQueryOpenCardPackResult(accepted:Boolean):void { queryResults.push(accepted); }
        public function onLevelUpButtonPressed():void { SetStageFocus(PickAPerk_mc); }
        public function FocusBase():void { SetStageFocus(Collection_mc); }
        public function SetStageFocus(value:MovieClip):void { stage.focus = value; }
        public function SetButtons():void { Object(this).B21SetButtons(); }
        public function SetPlatform(value:uint, gen9:Boolean, controller:uint, keyboard:uint):void {
            platform = value;
        }
    }
}
