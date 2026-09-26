Function ReconcileLocalEquipment(Actor akActor)
    If akActor != Game.GetPlayer()
        Return
    EndIf
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None
        masterScript.InitializeLocalEquipmentTracking()
    EndIf
EndFunction

Event OnEquipped(Actor akActor)
    ReconcileLocalEquipment(akActor)
EndEvent

Event OnUnequipped(Actor akActor)
    ReconcileLocalEquipment(akActor)
EndEvent
