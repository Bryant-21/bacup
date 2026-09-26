        public function ResolveIcon(data:Object):String
        {
            if (data.keywords != null)
                for each (var keyword:String in data.keywords)
                    if (keywordIcons[keyword] != null) return String(keywordIcons[keyword]);
            return data.icon == null ? "UnknownIcon" : String(data.icon);
        }
