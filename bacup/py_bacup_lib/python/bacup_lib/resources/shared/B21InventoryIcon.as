package
{
    import flash.display.Sprite;
    import flash.geom.ColorTransform;
    import flash.geom.Rectangle;
    import flash.utils.getDefinitionByName;
    public class B21InventoryIcon
    {
        // Native list icons share one square grid (rendered models are downsampled to it).
        public static const GRID:Number = 64;
        // Indexed by glyph id, shared with Tales InventoryIconRules.h and inventory_icons/catalog.json.
        private static const glyphs:Array = ["B21GlyphMisc", "B21GlyphWeapon", "B21GlyphApparel", "B21GlyphMedicine", "B21GlyphFood",
            "B21GlyphDrink", "B21GlyphArmor", "B21GlyphPowerArmor", "B21GlyphAmmo", "B21GlyphJunk",
            "B21GlyphMod", "B21GlyphBook", "B21GlyphHolotape", "B21GlyphPistol", "B21GlyphRifle",
            "B21GlyphShotgun", "B21GlyphHeavy", "B21GlyphLauncher", "B21GlyphEnergy", "B21GlyphBlade",
            "B21GlyphBlunt", "B21GlyphFist", "B21GlyphBow", "B21GlyphGrenade", "B21GlyphMine",
            "B21GlyphShells", "B21GlyphCell", "B21GlyphMissile", "B21GlyphArrow", "B21GlyphFusioncore",
            "B21GlyphHat", "B21GlyphHelmet", "B21GlyphEyewear", "B21GlyphMask", "B21GlyphGloves",
            "B21GlyphRing", "B21GlyphBackpack", "B21GlyphArmorarm", "B21GlyphArmorleg", "B21GlyphJumpsuit",
            "B21GlyphSyringe", "B21GlyphPills", "B21GlyphInhaler", "B21GlyphRadaway", "B21GlyphMeat",
            "B21GlyphProduce", "B21GlyphPackaged", "B21GlyphCooked", "B21GlyphWater", "B21GlyphSoda",
            "B21GlyphAlcohol", "B21GlyphMagazine", "B21GlyphNote", "B21GlyphKey", "B21GlyphKeycard",
            "B21GlyphBobbypin", "B21GlyphBobblehead", "B21GlyphCurrency", "B21GlyphPlan", "B21GlyphStar",
            "B21GlyphDevice", "B21GlyphMaterial"];
        private static const colors:Array = [0xe6dfc0, 0xa9bed2, 0xe9d6ae, 0xff6f6f, 0x93d46d, 0x72b8ff, 0xb9c5cc, 0xf2c94c, 0xd8b55b, 0xa7a7a7,
            0x6fb8da, 0x7fd2d2, 0x82b4ff, 0xb8c6d6, 0xa2b4c6, 0xa2b4c6, 0xd08e98, 0xd08e98, 0x9cc0ff, 0xf4c79c,
            0xf4c79c, 0xf4c79c, 0xc9a77a, 0xf58c94, 0xc95c68, 0xd8b55b, 0x5cd2ff, 0xd08e98, 0xc9a77a, 0xffd23f,
            0xe9d6ae, 0xb9c5cc, 0xe9d6ae, 0xb9c5cc, 0xe9d6ae, 0xf2c94c, 0xc9a77a, 0xb9c5cc, 0xb9c5cc, 0x6fb8da,
            0xff9a6b, 0xff9a6b, 0xff9a6b, 0xe5a84d, 0xe88a6a, 0x93d46d, 0xc8c8c8, 0xe3b46a, 0x72b8ff, 0xff6b6b,
            0xd9a05b, 0x7fd2d2, 0xe2e2e2, 0xf2c94c, 0xf2c94c, 0xcf8a4a, 0xf2c94c, 0xf2c94c, 0xc7a6ff, 0xffd700,
            0x8fd38f, 0xd4b26a];
        public static function kindOf(entry:Object):uint
        {
            var category:uint = entry.B21Category === undefined ? 4 : uint(entry.B21Category);
            var kind:uint = entry.B21IconType === undefined ? uint([0, 1, 2, 3, 0, 9, 10, 8][category]) : uint(entry.B21IconType);
            return kind < glyphs.length ? kind : 0;
        }
        // Rows opt into item type colors (Tales Config); a highlighted row keeps its own text color.
        public static function colorOf(entry:Object, color:uint, highlighted:Boolean):uint
        {
            return !highlighted && entry.B21IconColors === true ? uint(colors[kindOf(entry)]) : color;
        }
        public static function draw(layer:Sprite, entry:Object, x:Number, y:Number, width:Number, height:Number, color:uint):void
        {
            layer.cacheAsBitmap = true;
            var runs:Array = entry.B21IconRuns as Array;
            if (runs != null && runs.length)
            {
                // Fitted to the drawn pixels, so a tall front view fills the box as well as a long weapon.
                var minX:Number = GRID, minY:Number = GRID, maxX:Number = 0, maxY:Number = 0;
                var n:int;
                for (n = 0; n < runs.length; n += 4)
                {
                    minX = Math.min(minX, Number(runs[n])); maxX = Math.max(maxX, Number(runs[n]) + Number(runs[n + 2]));
                    minY = Math.min(minY, Number(runs[n + 1])); maxY = Math.max(maxY, Number(runs[n + 1]) + 1);
                }
                var scale:Number = Math.min(width / (maxX - minX), height / (maxY - minY));
                var left:Number = x + (width - (maxX - minX) * scale) / 2 - minX * scale;
                var top:Number = y + (height - (maxY - minY) * scale) / 2 - minY * scale;
                // GFx tessellates every fill style separately, so draw each alpha level as one fill.
                var levels:Object = {};
                for (n = 0; n < runs.length; n += 4)
                {
                    var level:Array = levels[runs[n + 3]];
                    if (level == null) levels[runs[n + 3]] = level = [];
                    level.push(n);
                }
                for (var alpha:String in levels)
                {
                    layer.graphics.beginFill(color, Number(alpha) / 255);
                    for each (var i:int in levels[alpha])
                        layer.graphics.drawRect(left + Number(runs[i]) * scale, top + Number(runs[i + 1]) * scale,
                            Number(runs[i + 2]) * scale, scale);
                    layer.graphics.endFill();
                }
                return;
            }
            var type:Class = getDefinitionByName(glyphs[kindOf(entry)]) as Class;
            var glyph:Sprite = new type() as Sprite;
            var bounds:Rectangle = glyph.getBounds(glyph);
            var fit:Number = Math.min(width / bounds.width, height / bounds.height);
            glyph.scaleX = glyph.scaleY = fit;
            glyph.x = x + (width - bounds.width * fit) / 2 - bounds.x * fit;
            glyph.y = y + (height - bounds.height * fit) / 2 - bounds.y * fit;
            glyph.transform.colorTransform = new ColorTransform(0, 0, 0, 1, (color >> 16) & 255, (color >> 8) & 255, color & 255);
            layer.addChild(glyph);
        }
    }
}
