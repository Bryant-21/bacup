; Jaye Vo's "Happily" lines (EquipDisguise) dress the player in a disguise; the "All done playing dress-up?" lines take
; it back.
Event OnEnd(ObjectReference akSpeakerRef, Bool abHasBeenSaid)
    Quests:GHL00:DisguiseVendorScript vendor = GetOwningQuest() as Quests:GHL00:DisguiseVendorScript
    If vendor == None
        Return
    EndIf
    If EquipDisguise
        vendor.EquipDisguise()
    Else
        vendor.RemoveDisguise()
    EndIf
EndEvent
