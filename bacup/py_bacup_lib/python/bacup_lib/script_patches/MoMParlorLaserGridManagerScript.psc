Function InitializeLocalEquipmentTracking()
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None
        masterScript.InitializeLocalEquipmentTracking()
    EndIf
EndFunction

Event OnInit()
    InitializeLocalEquipmentTracking()
EndEvent

Event OnLoad()
    InitializeLocalEquipmentTracking()
EndEvent
