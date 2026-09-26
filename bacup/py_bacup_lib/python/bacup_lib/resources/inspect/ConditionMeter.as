package
{
    import flash.display.MovieClip;
    import flash.geom.Rectangle;

    public class ExamineMenu extends MovieClip
    {
        public var entry:ItemCard_ItemHealthEntry = new ItemCard_ItemHealthEntry();
        public var renderedHealth:Number = -1;
        public var renderedDurability:Number = -1;
        public var renderedWidth:Number = -1;

        public function ExamineMenu()
        {
            super();
            mouseEnabled = false;
            mouseChildren = false;
            addChild(entry);
        }

        public function SetCondition(health:Number, durability:Number, width:Number):void
        {
            entry.PopulateEntry({currentHealth: health * 100, maximumHealth: 100, durability: durability});
            var bounds:Rectangle = entry.getBounds(entry);
            var scale:Number = bounds.width > 0 ? width / bounds.width : 1;
            entry.scaleX = scale;
            entry.scaleY = scale;
            entry.x = -bounds.x * scale;
            entry.y = -bounds.y * scale;
            renderedHealth = health;
            renderedDurability = durability;
            renderedWidth = width;
        }
    }
}
