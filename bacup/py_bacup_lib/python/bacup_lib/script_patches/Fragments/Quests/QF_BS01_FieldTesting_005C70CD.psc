Function Fragment_Stage_0100_Item_00()
    Actor playerRef = Game.GetPlayer()
    Book recommendationLetter = pBS01_MQ02_Invention_RecLetter_Bad

    If playerRef == None
        Return
    EndIf
    If Alias_Player != None && Alias_Player.GetReference() != playerRef
        Alias_Player.ForceRefTo(playerRef)
    EndIf
    If pBS01_MQ02_Invention_ValdezRep_AV != None && pBS01_MQ02_Invention_ValdezRep_UpsetThreshold != None && playerRef.GetValue(pBS01_MQ02_Invention_ValdezRep_AV) >= pBS01_MQ02_Invention_ValdezRep_UpsetThreshold.GetValue()
        recommendationLetter = pBS01_MQ02_Invention_RecLetter_Good
    EndIf
    If recommendationLetter != None && playerRef.GetItemCount(pBS01_MQ02_Invention_RecLetter_Good) == 0 && playerRef.GetItemCount(pBS01_MQ02_Invention_RecLetter_Bad) == 0
        playerRef.AddItem(recommendationLetter, 1, False)
    EndIf
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0150_Item_00()
    Actor playerRef = Game.GetPlayer()

    If playerRef != None
        If pBS01_MQ02_Invention_RecLetter_Good != None
            playerRef.RemoveItem(pBS01_MQ02_Invention_RecLetter_Good, playerRef.GetItemCount(pBS01_MQ02_Invention_RecLetter_Good), True)
        EndIf
        If pBS01_MQ02_Invention_RecLetter_Bad != None
            playerRef.RemoveItem(pBS01_MQ02_Invention_RecLetter_Bad, playerRef.GetItemCount(pBS01_MQ02_Invention_RecLetter_Bad), True)
        EndIf
    EndIf
    If Alias_Item_ValdezLetter != None
        Alias_Item_ValdezLetter.Clear()
    EndIf
    SetObjectiveCompleted(100)
    SetObjectiveDisplayed(200)
EndFunction

Function Fragment_Stage_0151_Item_00()
    Actor playerRef = Game.GetPlayer()

    If playerRef != None && Item_Stimpak != None
        playerRef.AddItem(Item_Stimpak, 1, False)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(200)
    SetObjectiveDisplayed(300)
    If Scene_Putnam_Intro != None && !Scene_Putnam_Intro.IsPlaying()
        Scene_Putnam_Intro.Start()
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(300)
    SetObjectiveDisplayed(400)
    SetObjectiveDisplayed(500)
    SetObjectiveDisplayed(600)
EndFunction

Function Fragment_Stage_0302_Item_00()
    Actor playerRef = Game.GetPlayer()

    If playerRef != None && Item_RadX != None
        playerRef.AddItem(Item_RadX, 1, False)
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    Actor playerRef = Game.GetPlayer()
    ObjectReference martyMarker

    If playerRef != None
        If BS01_RecruitedMarty != None
            playerRef.SetValue(BS01_RecruitedMarty, 1.0)
        EndIf
        If BS01_RecruitedColin != None
            playerRef.SetValue(BS01_RecruitedColin, 0.0)
        EndIf
    EndIf
    SetObjectiveCompleted(400)
    SetObjectiveFailed(500)
    If Alias_EnableMarker_MartyBunker != None
        martyMarker = Alias_EnableMarker_MartyBunker.GetReference()
    EndIf
    If martyMarker != None
        martyMarker.Enable()
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    Actor playerRef = Game.GetPlayer()
    ObjectReference colinMarker

    If playerRef != None
        If BS01_RecruitedColin != None
            playerRef.SetValue(BS01_RecruitedColin, 1.0)
        EndIf
        If BS01_RecruitedMarty != None
            playerRef.SetValue(BS01_RecruitedMarty, 0.0)
        EndIf
    EndIf
    SetObjectiveCompleted(500)
    SetObjectiveFailed(400)
    If Alias_EnableMarker_ColinBunker != None
        colinMarker = Alias_EnableMarker_ColinBunker.GetReference()
    EndIf
    If colinMarker != None
        colinMarker.Enable()
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    Actor playerRef = Game.GetPlayer()

    If playerRef != None
        If BS01_ReachedOrwell != None
            playerRef.SetValue(BS01_ReachedOrwell, 1.0)
        EndIf
        If (BS01_RecruitedMarty == None || playerRef.GetValue(BS01_RecruitedMarty) <= 0.0) && (BS01_RecruitedColin == None || playerRef.GetValue(BS01_RecruitedColin) <= 0.0)
            SetObjectiveFailed(400)
            SetObjectiveFailed(500)
        EndIf
    EndIf
    SetObjectiveCompleted(600)
    SetObjectiveDisplayed(700)
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(700)
    SetObjectiveDisplayed(800)
EndFunction

Function Fragment_Stage_0700_Item_01()
    ObjectReference bunkerTriggerMarker

    If Alias_EnableMarker_BunkerTriggers != None
        bunkerTriggerMarker = Alias_EnableMarker_BunkerTriggers.GetReference()
    EndIf
    If bunkerTriggerMarker != None
        bunkerTriggerMarker.Enable()
    EndIf
EndFunction

Function Fragment_Stage_0750_Item_00()
    Actor playerRef = Game.GetPlayer()
    ObjectReference companionMarker

    If playerRef != None && BS01_RecruitedMarty != None && playerRef.GetValue(BS01_RecruitedMarty) > 0.0 && Alias_EnableMarker_MartyBunker != None
        companionMarker = Alias_EnableMarker_MartyBunker.GetReference()
    ElseIf playerRef != None && BS01_RecruitedColin != None && playerRef.GetValue(BS01_RecruitedColin) > 0.0 && Alias_EnableMarker_ColinBunker != None
        companionMarker = Alias_EnableMarker_ColinBunker.GetReference()
    EndIf
    If companionMarker != None
        companionMarker.Enable()
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    Actor playerRef = Game.GetPlayer()
    Bool recruitedMarty = False
    Bool recruitedColin = False
    DefaultQuestEncounterWaveScript encounterController = (Self as Quest) as DefaultQuestEncounterWaveScript
    ObjectReference livingAreaSpawnCenter
    ObjectReference poolSpawnCenter

    SetObjectiveCompleted(800)
    If playerRef != None
        If BS01_RecruitedMarty != None
            recruitedMarty = playerRef.GetValue(BS01_RecruitedMarty) > 0.0
        EndIf
        If BS01_RecruitedColin != None
            recruitedColin = playerRef.GetValue(BS01_RecruitedColin) > 0.0
        EndIf
    EndIf
    If !recruitedMarty && !recruitedColin
        SetObjectiveDisplayed(1000)
        If Alias_SpawnCenter_Bunker_LivingArea != None
            livingAreaSpawnCenter = Alias_SpawnCenter_Bunker_LivingArea.GetReference()
        EndIf
        If Alias_SpawnCenter_Bunker_Pool != None
            poolSpawnCenter = Alias_SpawnCenter_Bunker_Pool.GetReference()
        EndIf
        If livingAreaSpawnCenter != None
            livingAreaSpawnCenter.Enable()
        EndIf
        If poolSpawnCenter != None
            poolSpawnCenter.Enable()
        EndIf
        If encounterController != None
            encounterController.StartBS01FieldTestingLocalEncounter()
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_01()
    Actor playerRef = Game.GetPlayer()

    If playerRef != None && BS01_RecruitedMarty != None && playerRef.GetValue(BS01_RecruitedMarty) > 0.0
        SetObjectiveDisplayed(900)
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_02()
    Actor playerRef = Game.GetPlayer()

    If playerRef != None && BS01_RecruitedColin != None && playerRef.GetValue(BS01_RecruitedColin) > 0.0
        SetObjectiveDisplayed(910)
    EndIf
EndFunction

Function Fragment_Stage_0810_Item_00()
    ObjectReference colinMarker

    If Alias_EnableMarker_ColinBunker != None
        colinMarker = Alias_EnableMarker_ColinBunker.GetReference()
    EndIf
    If colinMarker != None
        colinMarker.Disable()
    EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
    Actor playerRef = Game.GetPlayer()
    DefaultQuestEncounterWaveScript encounterController = (Self as Quest) as DefaultQuestEncounterWaveScript
    ObjectReference livingAreaSpawnCenter
    ObjectReference poolSpawnCenter

    If playerRef != None && BS01_RecruitedMarty != None && playerRef.GetValue(BS01_RecruitedMarty) > 0.0
        SetObjectiveCompleted(900)
    ElseIf playerRef != None && BS01_RecruitedColin != None && playerRef.GetValue(BS01_RecruitedColin) > 0.0
        SetObjectiveCompleted(910)
    EndIf
    SetObjectiveDisplayed(1000)
    If Alias_SpawnCenter_Bunker_LivingArea != None
        livingAreaSpawnCenter = Alias_SpawnCenter_Bunker_LivingArea.GetReference()
    EndIf
    If Alias_SpawnCenter_Bunker_Pool != None
        poolSpawnCenter = Alias_SpawnCenter_Bunker_Pool.GetReference()
    EndIf
    If livingAreaSpawnCenter != None
        livingAreaSpawnCenter.Enable()
    EndIf
    If poolSpawnCenter != None
        poolSpawnCenter.Enable()
    EndIf
    If encounterController != None
        encounterController.StartBS01FieldTestingLocalEncounter()
    EndIf
EndFunction

Function Fragment_Stage_0980_Item_00()
    ObjectReference livingAreaSpawnCenter

    If Alias_SpawnCenter_Bunker_LivingArea != None
        livingAreaSpawnCenter = Alias_SpawnCenter_Bunker_LivingArea.GetReference()
    EndIf
    If livingAreaSpawnCenter != None
        livingAreaSpawnCenter.Disable()
    EndIf
    If IsStageDone(980) && IsStageDone(990) && !IsStageDone(1000)
        SetStage(1000)
    EndIf
EndFunction

Function Fragment_Stage_0990_Item_00()
    ObjectReference poolSpawnCenter

    If Alias_SpawnCenter_Bunker_Pool != None
        poolSpawnCenter = Alias_SpawnCenter_Bunker_Pool.GetReference()
    EndIf
    If poolSpawnCenter != None
        poolSpawnCenter.Disable()
    EndIf
    If IsStageDone(980) && IsStageDone(990) && !IsStageDone(1000)
        SetStage(1000)
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    Actor playerRef = Game.GetPlayer()
    ObjectReference bunkerTriggerMarker
    Bool recruitedMarty = False
    Bool recruitedColin = False

    SetObjectiveCompleted(1000)
    If playerRef != None
        If BS01_RecruitedMarty != None
            recruitedMarty = playerRef.GetValue(BS01_RecruitedMarty) > 0.0
        EndIf
        If BS01_RecruitedColin != None
            recruitedColin = playerRef.GetValue(BS01_RecruitedColin) > 0.0
        EndIf
    EndIf
    If Alias_EnableMarker_BunkerTriggers != None
        bunkerTriggerMarker = Alias_EnableMarker_BunkerTriggers.GetReference()
    EndIf
    If bunkerTriggerMarker != None
        bunkerTriggerMarker.Disable()
    EndIf
    If !recruitedMarty && !recruitedColin
        SetObjectiveDisplayed(1200)
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_01()
    Actor playerRef = Game.GetPlayer()

    If playerRef != None && BS01_RecruitedMarty != None && playerRef.GetValue(BS01_RecruitedMarty) > 0.0
        If BS01_MartyPostBunker != None
            playerRef.SetValue(BS01_MartyPostBunker, 1.0)
        EndIf
        SetObjectiveDisplayed(1100)
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_02()
    Actor playerRef = Game.GetPlayer()
    ObjectReference colinMarker

    If playerRef != None && BS01_RecruitedColin != None && playerRef.GetValue(BS01_RecruitedColin) > 0.0
        If BS01_ColinPostBunker != None
            playerRef.SetValue(BS01_ColinPostBunker, 1.0)
        EndIf
        If Alias_EnableMarker_ColinBunker != None
            colinMarker = Alias_EnableMarker_ColinBunker.GetReference()
        EndIf
        If colinMarker != None
            colinMarker.Enable()
        EndIf
        SetObjectiveDisplayed(1110)
    EndIf
EndFunction

Function Fragment_Stage_1100_Item_00()
    Actor playerRef = Game.GetPlayer()

    If playerRef != None && BS01_RecruitedMarty != None && playerRef.GetValue(BS01_RecruitedMarty) > 0.0
        SetObjectiveCompleted(1100)
    ElseIf playerRef != None && BS01_RecruitedColin != None && playerRef.GetValue(BS01_RecruitedColin) > 0.0
        SetObjectiveCompleted(1110)
    EndIf
    SetObjectiveDisplayed(1200)
EndFunction

Function Fragment_Stage_9000_Item_00()
    Actor playerRef
    Bool handoffAccepted = False

    SetObjectiveCompleted(1200)
    If Alias_Player != None
        playerRef = Alias_Player.GetActorReference()
    EndIf
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef != None && BS01_AV_IsInitiate != None
        playerRef.SetValue(BS01_AV_IsInitiate, 1.0)
    EndIf
    If pBS01_MQ04_Arms_StartKeyword != None && playerRef != None
        handoffAccepted = pBS01_MQ04_Arms_StartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
        If !handoffAccepted
            StartTimer(5.0, 9000)
        EndIf
    EndIf
EndFunction

Event OnTimer(Int aiTimerID)
    Actor playerRef
    Bool handoffAccepted = False

    If aiTimerID != 9000 || !IsStageDone(9000)
        Return
    EndIf
    If Alias_Player != None
        playerRef = Alias_Player.GetActorReference()
    EndIf
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If pBS01_MQ04_Arms_StartKeyword != None && playerRef != None
        handoffAccepted = pBS01_MQ04_Arms_StartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
        If !handoffAccepted
            StartTimer(5.0, 9000)
        EndIf
    EndIf
EndEvent
