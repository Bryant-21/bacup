; Method fill for the hollow FO76 WindChimesActivatorScript. Supplies only the
; event body; the decompiled skeleton provides the Scriptname, Extends, and the
; ResourceToGive / ChimePickupSound properties. Original code — no game Papyrus.
; On activation: give the bound resource, play the pickup sound, and remove the
; chimes (the "Take" interaction).

Event OnActivate(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer()
        akActionRef.AddItem(ResourceToGive, 1)
        ChimePickupSound.Play(Self)
        Disable()
        Delete()
    EndIf
EndEvent
