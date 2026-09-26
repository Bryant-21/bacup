package
{
    import flash.display.Loader;
    import flash.display.MovieClip;
    import flash.events.Event;
    import flash.external.ExternalInterface;
    import flash.net.URLRequest;
    import flash.system.ApplicationDomain;
    import flash.system.LoaderContext;

    public class WorkshopPreview extends MovieClip
    {
        private var loader:Loader = new Loader();
        private var menu:Object;
        private var checks:Array = [];
        private var frames:int = 0;

        public function WorkshopPreview()
        {
            addChild(loader);
            loader.contentLoaderInfo.addEventListener(Event.COMPLETE, Loaded);
            loader.load(new URLRequest("Workshop.swf"),
                new LoaderContext(false, new ApplicationDomain(ApplicationDomain.currentDomain)));
        }

        private function Check(ok:Boolean, name:String):void
        {
            checks.push((ok ? "PASS " : "FAIL ") + name);
        }

        private function Loaded(event:Event):void
        {
            menu = Object(loader.content).Menu_mc;
            if (!menu) menu = Object(loader.content).Workshop_mc;
            if (!menu) menu = Object(loader.content).getChildAt(0);
            addEventListener(Event.ENTER_FRAME, Ready);
        }

        private function Ready(event:Event):void
        {
            frames++;
            if (!menu.B21ArtReady && frames < 180) return;
            removeEventListener(Event.ENTER_FRAME, Ready);
            Check(menu.B21PrototypeVersion == 1, "original FO4 menu has prototype extension");
            Check(menu.B21ArtReady, "converted FO76 artwork loaded");
            Check(menu.B21ArtError == "", "no artwork load error");
            menu.displayPath = "Structures;Wood;Floors";
            menu.displayName = "Wooden Floor";
            menu.requirements = [{text: "Wood", counts: "320/4", taggedForSearch: false},
                                 {text: "Steel", counts: "180/2", taggedForSearch: false}];
            menu.SetItemCount(3);
            menu.B21Refresh(null);
            Check(menu.DisplayPathBase_mc.DisplayPath_tf.text.toUpperCase().indexOf("FLOORS") >= 0, "native category path");
            Check(menu.ItemNameBase_mc.ItemName_tf.text.toUpperCase() == "WOODEN FLOOR", "native item selection");
            Check(menu.requirements.length == 2, "native material requirements retained");
            Check(menu.SelectionBracket_mc != null && menu.IconBackground_mc != null, "native 3D renderer anchors retained");
            Check(menu.ListInfoA.length == 3, "native row animation state retained");
            menu.peopleCount = 8;
            menu.foodCount = 12;
            menu.waterCount = 15;
            menu.powerCount = 20;
            menu.safetyCount = 25;
            menu.bedsCount = 8;
            menu.happyCount = 80;
            menu.happinessBarVisible = true;
            menu.size = 0.35;
            menu.SetValidDirections(true, true, true, true);
            menu.HidePerkPanels();
            menu.HideIconCard();
            menu.ButtonBackground_mc.x = 640;
            menu.ButtonBackground_mc.y = 690;
            menu.SetButtonText(true, false, false, false, false, false, false, false,
                false, false, false, false, false, false, false, false, false, false,
                false, false, false, false, 0);
            var failed:int = 0;
            for each (var check:String in checks) if (check.indexOf("FAIL ") == 0) failed++;
            ExternalInterface.call("report", (checks.length - failed) + " passed; " + failed + " failed\n" +
                "Browser cannot render the engine's 3D previews. Verify those in game.\n" + checks.join("\n"));
        }
    }
}
