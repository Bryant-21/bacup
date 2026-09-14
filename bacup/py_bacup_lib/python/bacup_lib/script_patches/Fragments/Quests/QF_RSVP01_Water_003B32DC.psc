Function Fragment_Stage_0010_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If !playerRef
        Alias_Player.ForceRefIfEmpty(Game.GetPlayer())
        playerRef = Alias_Player.GetActorReference()
    EndIf
    If playerRef
        playerRef.SetValue(pRSVP00_AV_StartedRSVP01, 1.0)
        playerRef.SetValue(AV_StartedQuest, 1.0)
        playerRef.SetValue(pRSVP00_AV_isVolunteerCandidate, 1.0)
    EndIf
    If !IsStageDone(300)
        SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(AV_QuestTracker, 300.0)
    EndIf
    SetObjectiveDisplayed(300)
    SetObjectiveDisplayed(310)
EndFunction

Function Fragment_Stage_0310_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(pRSVP01_AV_UsedTerminal, 1.0)
    EndIf
    SetObjectiveCompleted(310)
    SetObjectiveDisplayed(300, False)
    SetObjectiveDisplayed(320)
EndFunction

Function Fragment_Stage_0320_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(pRSVP00_AV_foundKesha, 1.0)
    EndIf
    SetObjectiveCompleted(300)
    SetObjectiveCompleted(320)
    SetObjectiveDisplayed(310, False)
    If !IsStageDone(400)
        SetStage(400)
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(AV_QuestTracker, 400.0)
    EndIf
    SetObjectiveDisplayed(400)
EndFunction

Function Fragment_Stage_0500_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(AV_QuestTracker, 500.0)
    EndIf
    SetObjectiveCompleted(400)
    If !IsStageDone(600)
        SetStage(600)
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(AV_QuestTracker, 600.0)
    EndIf
    SetObjectiveDisplayed(605)
    SetObjectiveDisplayed(610)
    If pRSVP01_Message_CollectWater
        pRSVP01_Message_CollectWater.Show()
    EndIf
EndFunction

Function Fragment_Stage_0605_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(pRSVP01_AV_RiverSampleCollected, 1.0)
    EndIf
    SetObjectiveCompleted(605)
    If Message_SampleTested
        Message_SampleTested.Show()
    EndIf
    If playerRef && playerRef.GetValue(PRSVP01_AV_WellSampleCollected) > 0.0 && !IsStageDone(700)
        SetStage(700)
    EndIf
EndFunction

Function Fragment_Stage_0610_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(PRSVP01_AV_WellSampleCollected, 1.0)
    EndIf
    SetObjectiveCompleted(610)
    If Message_SampleTested
        Message_SampleTested.Show()
    EndIf
    If playerRef && playerRef.GetValue(pRSVP01_AV_RiverSampleCollected) > 0.0 && !IsStageDone(700)
        SetStage(700)
    EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(AV_QuestTracker, 700.0)
    EndIf
    SetObjectiveCompleted(605)
    SetObjectiveCompleted(610)
    SetObjectiveDisplayed(700)
EndFunction

Function Fragment_Stage_0800_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(AV_QuestTracker, 800.0)
        playerRef.SetValue(pRSVP01_AV_AnalyzedSample, 1.0)
    EndIf
    SetObjectiveCompleted(700)
    SetObjectiveDisplayed(800)
    If pRSVP01_Message_AnalyzingData
        pRSVP01_Message_AnalyzingData.Show()
    EndIf
    If pRSVP01_Message_ReturnWaterTestingKit
        pRSVP01_Message_ReturnWaterTestingKit.Show()
    EndIf
    If playerRef && pWaterBoiled
        If playerRef.GetItemCount(pWaterBoiled) == 0
            playerRef.AddItem(pWaterBoiled, 1, False)
        EndIf
        If playerRef.GetItemCount(pWaterBoiled) > 0 && !IsStageDone(900)
            SetStage(900)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(AV_QuestTracker, 900.0)
    EndIf
    SetObjectiveCompleted(800)
    SetObjectiveDisplayed(810, False)
    SetObjectiveDisplayed(820, False)
    SetObjectiveDisplayed(900)
EndFunction

Function Fragment_Stage_1000_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(AV_QuestTracker, 1000.0)
    EndIf
    SetObjectiveCompleted(900)
    SetObjectiveDisplayed(1000)
EndFunction

Function Fragment_Stage_8999_Item_00()
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    SetObjectiveCompleted(1000)
    If playerRef
        playerRef.SetValue(AV_QuestDone, 1.0)
        playerRef.SetValue(RSVP02_AV_QuestStarted, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_9500_Item_00()
    Stop()
EndFunction

Function Fragment_Stage_9999_Item_00()
    Stop()
EndFunction
