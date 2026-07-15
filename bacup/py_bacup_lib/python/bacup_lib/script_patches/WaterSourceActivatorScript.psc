; Method fill for the hollow FO76 WaterSourceActivatorScript. Supplies only the
; event body; the decompiled skeleton provides the Scriptname, Extends, and the
; DrinkingSpell property. Original code — no game Papyrus.
; On activation: cast the bound drinking spell on the player (the FO76 water
; "drink" effect converts to a FO4 SPEL), giving the dirty-water effect.

Event OnActivate(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer()
        DrinkingSpell.Cast(Self, akActionRef)
    EndIf
EndEvent
