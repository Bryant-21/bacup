package
{
    public class B21InventoryRules
    {
        private static var sortMode:uint;
        private static var sortDescending:Boolean;
        public static function project(source:Array, mode:uint, descending:Boolean, query:String):Array
        {
            var result:Array = [];
            var words:Array = (query == null ? "" : query.toLowerCase()).split(" ");
            for (var i:int = 0; i < source.length; i++)
            {
                var entry:Object = source[i];
                entry.B21OriginalIndex = i;
                var name:String = String(entry.text == null ? entry.name : entry.text).toLowerCase();
                var matches:Boolean = true;
                for each (var word:String in words)
                    if (word.length && name.indexOf(word) < 0) matches = false;
                if (matches) result.push(entry);
            }
            if (mode != 0)
            {
                sortMode = mode;
                sortDescending = descending;
                result.sort(compare);
            }
            return result;
        }

        private static function compare(a:Object, b:Object):Number
        {
            var left:Object = key(a, sortMode);
            var right:Object = key(b, sortMode);
            // Missing values stay last in either direction; zero-weight value is unbounded.
            var missingA:Boolean = left == null;
            var missingB:Boolean = right == null;
            if (missingA != missingB) return missingA ? 1 : -1;
            var difference:Number = left < right ? -1 : left > right ? 1 : 0;
            if (difference != 0) return sortDescending ? -difference : difference;
            return int(a.B21OriginalIndex) - int(b.B21OriginalIndex);
        }

        public static function key(entry:Object, mode:uint):Object
        {
            if (mode == 1) return String(entry.text == null ? entry.name : entry.text).toLowerCase();
            var weight:Number = entry.B21Weight === undefined ? NaN : Number(entry.B21Weight);
            var value:Number = entry.B21Value === undefined ? NaN : Number(entry.B21Value);
            if (mode == 2) return isNaN(weight) || weight < 0 ? null : weight;
            if (mode == 3) return isNaN(value) || value < 0 ? null : value;
            if (isNaN(weight) || weight < 0 || isNaN(value) || value < 0) return null;
            return weight == 0 ? (value > 0 ? Number.POSITIVE_INFINITY : 0) : value / weight;
        }

        public static function nativeIndex(view:Array, index:int):int
        {
            return index < 0 || index >= view.length ? -1 : int(view[index].B21OriginalIndex);
        }

        public static function viewIndex(view:Array, index:int):int
        {
            for (var i:int = 0; i < view.length; i++)
                if (int(view[i].B21OriginalIndex) == index) return i;
            return -1;
        }

        public static function number(value:Object, decimals:uint = 0):String
        {
            if (value == null || isNaN(Number(value))) return "--";
            if (Number(value) == Number.POSITIVE_INFINITY) return "--";
            var result:String = Number(value).toFixed(decimals);
            while (result.indexOf(".") >= 0 && result.charAt(result.length - 1) == "0") result = result.substr(0, result.length - 1);
            if (result.charAt(result.length - 1) == ".") result = result.substr(0, result.length - 1);
            return result;
        }
    }
}
