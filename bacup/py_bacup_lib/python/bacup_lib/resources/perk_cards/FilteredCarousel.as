package {
    import Shared.AS3.BSUIComponent;
    public class FilteredCarousel extends BSUIComponent {
        public function B21FilterText(value:String):String {
            return value == "$FILTER" ? "$B21_TFA_PerkCards_FilterAll" : value;
        }
    }
}
