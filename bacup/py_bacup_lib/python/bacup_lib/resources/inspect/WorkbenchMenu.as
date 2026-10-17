package
{
    import Shared.IMenu;
    import Shared.AS3.BSButtonHintData;

    public class ExamineMenu extends IMenu
    {
        public var B21RepairReady:Boolean;
        private var B21RepairHint:BSButtonHintData;

        public function B21SetRepairHint(show:Boolean, callback:Function):void
        {
            var changed:Boolean = B21RepairHint == null || B21RepairHint.ButtonVisible != show;
            if (B21RepairHint == null)
            {
                B21RepairHint = new BSButtonHintData("$B21_TFA_Repair", "K", "PSN_L3", "Xenon_L3", 1, callback);
                InventoryButtonHints.push(B21RepairHint);
                ModSlotButtonHints.push(B21RepairHint);
                InspectModeButtons.push(B21RepairHint);
                B21RepairReady = true;
            }
            B21RepairHint.ButtonVisible = show;
            if (changed)
            {
                if (_inspectMode) ButtonHintBar_mc.SetButtonHintData(InspectModeButtons);
                else UpdateButtons();
            }
        }

        public var B21RenameReady:Boolean;
        private var B21RenameHint:BSButtonHintData;

        // The workbench has its own RenameButton; this one is Inspect-only.
        public function B21SetRenameHint(show:Boolean, callback:Function):void
        {
            var changed:Boolean = B21RenameHint == null || B21RenameHint.ButtonVisible != show;
            if (B21RenameHint == null)
            {
                B21RenameHint = new BSButtonHintData("$RENAME", "V", "PSN_Y", "Xenon_Y", 1, callback);
                InspectModeButtons.push(B21RenameHint);
                B21RenameReady = true;
            }
            B21RenameHint.ButtonVisible = show;
            if (changed && _inspectMode) ButtonHintBar_mc.SetButtonHintData(InspectModeButtons);
        }

        private var B21ModeHint:BSButtonHintData;
        private var B21LockHint:BSButtonHintData;
        private var B21ScrapHints:Vector.<BSButtonHintData>;
        public var B21ScrapMode:Boolean;

        public function B21SetLockHint(show:Boolean, locked:Boolean, callback:Function):void
        {
            if (B21LockHint == null)
            {
                B21LockHint = new BSButtonHintData("$B21_TFA_Lock", "L", "", "", 1, callback);
                InventoryButtonHints.push(B21LockHint);
                InspectModeButtons.push(B21LockHint);
            }
            var label:String = locked ? "$B21_TFA_Unlock" : "$B21_TFA_Lock";
            var changed:Boolean = B21LockHint.ButtonVisible != show || B21LockHint.ButtonText != label;
            B21LockHint.ButtonVisible = show;
            B21LockHint.ButtonText = label;
            if (changed)
            {
                if (_inspectMode) ButtonHintBar_mc.SetButtonHintData(InspectModeButtons);
                else UpdateButtons();
            }
        }

        // Tales calls this every frame. AE's ButtonBarMenu forwarder reads the list's length and
        // every entry natively, so a null list or entry is a CTD: build the list before anything
        // can throw, and mark the mode only once its setup has completed.
        public function B21SetScrapMode(callback:Function):void
        {
            if (B21ScrapHints == null)
            {
                var hints:Vector.<BSButtonHintData> = new Vector.<BSButtonHintData>();
                hints.push(new BSButtonHintData("$SCRAP", "Space", "PSN_A", "Xenon_A", 1, onScrapBuildAdd));
                hints.push(new BSButtonHintData("$B21_TFA_ScrapAllJunk", "T", "", "", 1, callback));
                if (B21LockHint != null) hints.push(B21LockHint);
                hints.push(BackButton);
                B21ScrapHints = hints;
            }
            if (!B21ScrapMode)
            {
                allowEquip = false;
                allowRename = false;
                allowRepair = false;
                showScrapButton = true;
                eMode = INVENTORY_MODE;
                B21ScrapMode = true;
            }
            ButtonHintBar_mc.SetButtonHintData(B21ScrapHints);
        }

        public function B21OnItemPressed():void
        {
            if (B21ScrapMode) onScrapBuildAdd();
            else onModButton();
        }

        // The recipe list is CookingMenu's first list, shown with the slot-mode buttons.
        public function B21SetModeHint(label:String, crafting:Boolean, callback:Function):void
        {
            if (B21ModeHint != null) return;
            B21ModeHint = new BSButtonHintData(label, "Q", "PSN_Select", "Xenon_Select", 1, callback);
            if (crafting) ModSlotButtonHints.push(B21ModeHint);
            else InventoryButtonHints.push(B21ModeHint);
            UpdateButtons();
        }
    }
}
