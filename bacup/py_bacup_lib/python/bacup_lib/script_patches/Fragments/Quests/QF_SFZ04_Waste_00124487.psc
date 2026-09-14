Function Fragment_Stage_0000_Item_00()
    Actor playerRef
    If Alias_SFZ04Player != None
        playerRef = Alias_SFZ04Player.GetActorReference()
    EndIf

    If playerRef != None && SFZ04_Waste_QuestCompletedValue != None && playerRef.GetValue(SFZ04_Waste_QuestCompletedValue) > 0.0
        SetStage(100)
    Else
        If SFZ04_Recycle_MaintenanceMessage != None
            SFZ04_Recycle_MaintenanceMessage.Start()
        EndIf
        SetStage(50)
    EndIf
EndFunction

Function Fragment_Stage_0050_Item_00()
    SetObjectiveDisplayed(50, True)
EndFunction

Function Fragment_Stage_0100_Item_00()
    If IsObjectiveDisplayed(50)
        SetObjectiveCompleted(50, True)
    EndIf
    SetObjectiveDisplayed(100, True)
EndFunction

Function Fragment_Stage_0110_Item_00()
    Return
EndFunction

Function Fragment_Stage_0115_Item_00()
    SFZ04_Waste_QuestScript questScript = (Self as Quest) as SFZ04_Waste_QuestScript
    If questScript != None
        questScript.AssignCoreToProtectron(Alias_SpawnedProtectron01)
    EndIf
EndFunction

Function Fragment_Stage_0120_Item_00()
    Return
EndFunction

Function Fragment_Stage_0125_Item_00()
    SFZ04_Waste_QuestScript questScript = (Self as Quest) as SFZ04_Waste_QuestScript
    If questScript != None
        questScript.AssignCoreToProtectron(Alias_SpawnedProtectron02)
    EndIf
EndFunction

Function Fragment_Stage_0130_Item_00()
    Return
EndFunction

Function Fragment_Stage_0135_Item_00()
    SFZ04_Waste_QuestScript questScript = (Self as Quest) as SFZ04_Waste_QuestScript
    If questScript != None
        questScript.AssignCoreToProtectron(Alias_SpawnedProtectron03)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(100, True)
    SetObjectiveDisplayed(200, True)
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveCompleted(200, True)

    Actor playerRef
    If Alias_SFZ04Player != None
        playerRef = Alias_SFZ04Player.GetActorReference()
    EndIf
    If playerRef != None && SFZ04_Waste_QuestCompletedValue != None
        playerRef.SetValue(SFZ04_Waste_QuestCompletedValue, 1.0)
    EndIf
    Stop()
EndFunction
