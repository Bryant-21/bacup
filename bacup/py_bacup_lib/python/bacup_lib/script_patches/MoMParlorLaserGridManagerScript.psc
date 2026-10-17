Function InitializeLocalEquipmentTracking()
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None
        masterScript.InitializeLocalEquipmentTracking()
    EndIf
EndFunction

Event OnInit()
    InitializeLocalEquipmentTracking()
EndEvent

; FO76 docstring: "updates the laser grids upon request, when players equip or unequip
; their veils while in the cell." The grids hang off this marker as a linked-ref chain,
; so this works even when MoMMaster's RiversideManorLaserGrids collection filled empty
; on a save that started the quest before the grids were converted.
Event OnLoad()
    InitializeLocalEquipmentTracking()
    Actor playerRef = Game.GetPlayer()
    RegisterForRemoteEvent(playerRef, "OnItemEquipped")
    RegisterForRemoteEvent(playerRef, "OnItemUnequipped")
    UpdateLaserGrids()
EndEvent

Event OnUnload()
    Actor playerRef = Game.GetPlayer()
    UnregisterForRemoteEvent(playerRef, "OnItemEquipped")
    UnregisterForRemoteEvent(playerRef, "OnItemUnequipped")
EndEvent

Event Actor.OnItemEquipped(Actor akSender, Form akBaseObject, ObjectReference akReference)
    If akSender == Game.GetPlayer() && akBaseObject as Armor
        UpdateLaserGrids()
    EndIf
EndEvent

Event Actor.OnItemUnequipped(Actor akSender, Form akBaseObject, ObjectReference akReference)
    If akSender == Game.GetPlayer() && akBaseObject as Armor
        UpdateLaserGrids()
    EndIf
EndEvent

Function UpdateLaserGrids()
    laserGridsNeedUpdate = True
    If lock_UpdateLaserGrids
        Return
    EndIf
    lock_UpdateLaserGrids = True
    While laserGridsNeedUpdate
        laserGridsNeedUpdate = False
        ; The item manager keeps MoMMistressOfMysteryFaction in step with the worn veil;
        ; the grids' RequiredFaction reads it.
        MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
        If masterScript != None
            masterScript.ReconcileLocalEquipment()
        EndIf
        Actor playerRef = Game.GetPlayer()
        ObjectReference grid = GetLinkedRef()
        Int guard = 0
        While grid != None && guard < 16
            LaserGridScript laserGrid = grid as LaserGridScript
            If laserGrid != None
                If laserGrid.PassesAccessConditions(playerRef)
                    If !grid.IsDisabled()
                        grid.DisableNoWait()
                    EndIf
                ElseIf grid.IsDisabled()
                    grid.EnableNoWait()
                EndIf
            EndIf
            grid = grid.GetLinkedRef()
            guard += 1
        EndWhile
    EndWhile
    lock_UpdateLaserGrids = False
EndFunction
