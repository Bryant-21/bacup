package SpecialBuilds {
    import flash.display.MovieClip;

    public class EditSpecialModal extends MovieClip {
        public function PreviewState():Object {
            return {active:this.isActive, label:currentLabel, selected:this.List_mc.selectedIndex,
                specials:this.List_mc.entryData, points:this.AvailablePoints_tf.text};
        }
    }
}
