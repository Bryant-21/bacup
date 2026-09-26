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
