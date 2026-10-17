; Radiation Rumble heavy-radiation zones (E05_Radiation_RadZoneTrigger01-04). While the event runs, entering one
; warns the player, applies the zone's image-space spell and starts the rads-tracking misc quest that points at
; the decontamination room; leaving removes the image space.
Event OnTriggerEnter(ObjectReference akActionRef)
    Actor player = Game.GetPlayer()
    If akActionRef != player || !EventRunning()
        Return
    EndIf
    If E05_Radiation_HeavyRadZoneMessage != None
        E05_Radiation_HeavyRadZoneMessage.Show()
    EndIf
    If IMODSpell != None
        IMODSpell.Cast(player, player)
    EndIf
    Quest radsTracker = Game.GetFormFromFile(0x58BCFE, "SeventySix.esm") as Quest
    If radsTracker != None && !radsTracker.IsRunning()
        radsTracker.Start()
    EndIf
EndEvent

Event OnTriggerLeave(ObjectReference akActionRef)
    Actor player = Game.GetPlayer()
    If akActionRef == player && IMODSpell != None
        player.DispelSpell(IMODSpell)
    EndIf
EndEvent

Bool Function EventRunning()
    If RadiationQuest == None
        RadiationQuest = E05_Radiation
    EndIf
    Return RadiationQuest != None && RadiationQuest.IsRunning()
EndFunction
