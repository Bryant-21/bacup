Function Fragment_Stage_0000_Item_00()
    If Alias_Player
        Alias_Player.ForceRefIfEmpty(Game.GetPlayer())
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    If RSVP03_MiscPointer != None && RSVP03_MiscPointer.IsRunning() && !RSVP03_MiscPointer.IsStageDone(100)
        RSVP03_MiscPointer.SetStage(100)
    EndIf
    If GetStage() <= 100
        SetObjectiveDisplayed(1000)
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveDisplayed(1000)
EndFunction

Function Fragment_Stage_1000_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef && pRSVP03_AV_GotHolotape
        playerRef.SetValue(pRSVP03_AV_GotHolotape, 1.0)
    EndIf
    If !IsStageDone(1100)
        SetObjectiveDisplayed(1000)
    EndIf
EndFunction

Function Fragment_Stage_1100_Item_00()
    SetObjectiveCompleted(1000)
    SetObjectiveDisplayed(1100)
EndFunction

Function Fragment_Stage_1110_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef && pRSVP03_AV_SpawnEncWave0
        playerRef.SetValue(pRSVP03_AV_SpawnEncWave0, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_1200_Item_00()
    SetObjectiveCompleted(1100)
    SetObjectiveDisplayed(1200)
EndFunction

Function Fragment_Stage_1225_Item_00()
    SetObjectiveCompleted(1200)
    If IsStageDone(1250)
        If !IsStageDone(1300)
            SetStage(1300)
        EndIf
    Else
        SetObjectiveDisplayed(1250)
    EndIf
EndFunction

Function Fragment_Stage_1250_Item_00()
    SetObjectiveCompleted(1250)
    If IsStageDone(1225)
        If !IsStageDone(1300)
            SetStage(1300)
        EndIf
    Else
        SetObjectiveDisplayed(1200)
    EndIf
EndFunction

Function Fragment_Stage_1300_Item_00()
    SetObjectiveCompleted(1200)
    SetObjectiveCompleted(1250)
    If !IsStageDone(1325)
        SetObjectiveDisplayed(1300)
    EndIf
    If !IsStageDone(1350)
        SetObjectiveDisplayed(1350)
    EndIf
EndFunction

Function Fragment_Stage_1310_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef && pRSVP03_AV_SpawnEncWave1
        playerRef.SetValue(pRSVP03_AV_SpawnEncWave1, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_1325_Item_00()
    SetObjectiveCompleted(1300)
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef && pRSVP03_AV_GotSchematics
        playerRef.SetValue(pRSVP03_AV_GotSchematics, 1.0)
        If pRSVP03_AV_GotProgram && playerRef.GetValue(pRSVP03_AV_GotProgram) > 0.0 && !IsStageDone(1400)
            SetStage(1400)
        EndIf
    EndIf
    If !IsStageDone(1350) && !IsStageDone(1400)
        SetObjectiveDisplayed(1350)
    EndIf
EndFunction

Function Fragment_Stage_1350_Item_00()
    SetObjectiveCompleted(1350)
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef && pRSVP03_AV_GotProgram
        playerRef.SetValue(pRSVP03_AV_GotProgram, 1.0)
        If pRSVP03_AV_GotSchematics && playerRef.GetValue(pRSVP03_AV_GotSchematics) > 0.0 && !IsStageDone(1400)
            SetStage(1400)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_1400_Item_00()
    SetObjectiveCompleted(1350)
    SetObjectiveDisplayed(1400)
EndFunction

Function Fragment_Stage_1410_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef && pRSVP03_AV_SpawnEncWave2
        playerRef.SetValue(pRSVP03_AV_SpawnEncWave2, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_1500_Item_00()
    SetObjectiveCompleted(1400)
EndFunction

Function Fragment_Stage_2000_Item_00()
    SetObjectiveCompleted(1400)
    SetObjectiveDisplayed(2000)
    If Scene_DeployCamp
        Scene_DeployCamp.Start()
    EndIf
EndFunction

Function Fragment_Stage_2100_Item_00()
    SetObjectiveCompleted(2000)
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef && playerRef.GetValue(pRSVP03_AV_StashObjective) > 0.0
        SetObjectiveDisplayed(2200)
        SetObjectiveDisplayed(2300)
    Else
        SetObjectiveDisplayed(2400)
        If Scene_BuildGenerator
            Scene_BuildGenerator.Start()
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_2200_Item_00()
    SetObjectiveCompleted(2200)
    If IsStageDone(2300) && !IsStageDone(2500)
        SetStage(2500)
    EndIf
EndFunction

Function Fragment_Stage_2300_Item_00()
    SetObjectiveCompleted(2300)
    If IsStageDone(2200) && !IsStageDone(2500)
        SetStage(2500)
    EndIf
EndFunction

Function Fragment_Stage_2400_Item_00()
    SetObjectiveCompleted(2400)
    If !IsStageDone(2500)
        SetStage(2500)
    EndIf
EndFunction

Function Fragment_Stage_2500_Item_00()
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(pRSVP03_AV_QuestCompletion, 1.0)
    EndIf
    If Scene_QuestComplete
        Scene_QuestComplete.Start()
    EndIf
    ; Safe for Work (node 50A585) requires RSVP01-03 completed, so complete
    ; before handing off through its K1 selector.
    CompleteQuest()
    If RSVP04_Keyword_QuestActive_Patrol
        RSVP04_Keyword_QuestActive_Patrol.SendStoryEvent(None, playerRef)
    EndIf
EndFunction

Function Fragment_Stage_9500_Item_00()
    Stop()
EndFunction

Function Fragment_Stage_9999_Item_00()
    Stop()
EndFunction
