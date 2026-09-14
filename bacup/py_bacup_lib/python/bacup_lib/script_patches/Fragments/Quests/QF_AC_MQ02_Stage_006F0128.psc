Actor Function ActorFromAlias(ReferenceAlias actorAlias)
    If actorAlias == None
        Return None
    EndIf
    Return actorAlias.GetActorReference()
EndFunction

ObjectReference Function PlayerReference()
    Return Alias_Player.GetReference()
EndFunction

Quests:AC_MQ02_Stage:QuestScript Function QuestController()
    Return (Self as Quest) as Quests:AC_MQ02_Stage:QuestScript
EndFunction

Function StartSceneIfStopped(Scene sceneToStart)
    If sceneToStart != None && !sceneToStart.IsPlaying()
        sceneToStart.Start()
    EndIf
EndFunction

Function StopSceneIfPlaying(Scene sceneToStop)
    If sceneToStop != None && sceneToStop.IsPlaying()
        sceneToStop.Stop()
    EndIf
EndFunction

Function EnableAlias(ReferenceAlias targetAlias)
    If targetAlias != None
        ObjectReference target = targetAlias.GetReference()
        If target != None
            target.Enable()
            Actor targetActor = target as Actor
            If targetActor != None
                targetActor.EvaluatePackage()
            EndIf
        EndIf
    EndIf
EndFunction

Function EnableCollection(RefCollectionAlias targets)
    If targets == None
        Return
    EndIf
    Int index = 0
    While index < targets.GetCount()
        ObjectReference target = targets.GetAt(index)
        If target != None
            target.Enable()
            Actor targetActor = target as Actor
            If targetActor != None
                targetActor.EvaluatePackage()
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction

Function PlayAudienceIdle(Idle reactionIdle)
    Int index = 0
    While index < Alias_Actors_Audience.GetCount()
        Actor audienceMember = Alias_Actors_Audience.GetAt(index) as Actor
        If audienceMember != None
            audienceMember.PlayIdle(reactionIdle)
        EndIf
        index += 1
    EndWhile
EndFunction

Function GiveDevilsBloodIfMissing()
    ObjectReference player = PlayerReference()
    If player != None && player.GetItemCount(AC_MQ02_Stage_DevilsBloodVial) < 1
        player.AddItem(AC_MQ02_Stage_DevilsBloodVial, 1, True)
    EndIf
EndFunction

Function RemoveDevilsBlood()
    ObjectReference player = PlayerReference()
    If player != None
        Int count = player.GetItemCount(AC_MQ02_Stage_DevilsBloodVial)
        If count > 0
            player.RemoveItem(AC_MQ02_Stage_DevilsBloodVial, count, True)
        EndIf
    EndIf
EndFunction

Function TryFinishDeadClowns()
    If IsStageDone(1830) && IsStageDone(1840) && !IsStageDone(1850)
        SetStage(1850)
    EndIf
EndFunction

Function Fragment_Stage_0010_Item_00()
    EnableAlias(Alias_Actor_Orlando)
    EnableAlias(Alias_Actor_Evelyn_Whitespring)
EndFunction

Function Fragment_Stage_0020_Item_00()
    If IsStageDone(500) && !IsStageDone(600)
        SetStage(600)
    EndIf
EndFunction

Function Fragment_Stage_0030_Item_00()
    EnableAlias(Alias_Actor_Stan_Casino)
EndFunction

Function Fragment_Stage_0040_Item_00()
    EnableAlias(Alias_Actor_TicketClerk)
EndFunction

Function Fragment_Stage_0050_Item_00()
    EnableAlias(Alias_Actor_Zayde)
EndFunction

Function Fragment_Stage_0060_Item_00()
    If IsStageDone(2800) && !IsStageDone(2850)
        SetStage(2850)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    ObjectReference player = PlayerReference()
    If XPD_Hub_Responders != None && XPD_Hub_Responders.IsCompleted()
        If !IsStageDone(150)
            SetStage(150)
        EndIf
        Return
    EndIf
    SetObjectiveDisplayed(5)
    If player != None && XPD_Hub_Responders != None && XPD_Hub_Responders_QuestStartKeyword != None && !XPD_Hub_Responders.IsRunning() && !player.HasKeyword(XPD_Hub_Responders_QuestActiveKeyword)
        XPD_Hub_Responders_QuestStartKeyword.SendStoryEvent(None, player, player)
    EndIf
EndFunction

Function Fragment_Stage_0150_Item_00()
    SetObjectiveCompleted(5)
    SetObjectiveDisplayed(10)
    EnableAlias(Alias_Actor_Orlando)
    EnableAlias(Alias_Actor_Evelyn_Whitespring)
EndFunction

Function Fragment_Stage_0200_Item_00()
    EnableAlias(Alias_Actor_Orlando)
    EnableAlias(Alias_Actor_OrlandoOfficeRefugee)
    StartSceneIfStopped(AC_MQ02_Stage_IntroAmbient)
EndFunction

Function Fragment_Stage_0250_Item_00()
    StopSceneIfPlaying(AC_MQ02_Stage_IntroAmbient)
    EnableAlias(Alias_Actor_Orlando)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(30)
    SetObjectiveDisplayed(40)
EndFunction

Function Fragment_Stage_0340_Item_00()
    EnableAlias(Alias_Actor_SuperstitiousRefugee)
    StartSceneIfStopped(AC_MQ02_Stage_SuperstitiousRefugee_Scene)
EndFunction

Function Fragment_Stage_0350_Item_00()
    EnableAlias(Alias_Actor_SuperstitiousRefugee)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(40)
    StopSceneIfPlaying(AC_MQ02_Stage_SuperstitiousRefugee_Scene)
EndFunction

Function Fragment_Stage_0500_Item_00()
    EnableCollection(Alias_Markers_LandingSite)
EndFunction

Function Fragment_Stage_0599_Item_00()
    If !IsStageDone(600)
        SetStage(600)
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(30)
    SetObjectiveCompleted(40)
    SetObjectiveDisplayed(50)
    EnableAlias(Alias_Actor_Evelyn_CasinoQuarter)
    EnableAlias(Alias_Door_BoardwalkToPier)
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(50)
    SetObjectiveDisplayed(60)
    EnableAlias(Alias_Actor_Stan_Casino)
    StartSceneIfStopped(AC_MQ02_Stage_EvelynExitCasinoQuarter)
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveCompleted(60)
    SetObjectiveDisplayed(70)
    SetObjectiveDisplayed(75)
    EnableCollection(Alias_Actors_HighRollersLoungeMobsters)
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveCompleted(70)
    StartSceneIfStopped(AC_MQ02_Stage_PokerGame)
EndFunction

Function Fragment_Stage_0950_Item_00()
    EnableAlias(Alias_Actor_HonestWiseGuy)
    EnableAlias(Alias_Actor_ShadyWiseGuy)
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveCompleted(75)
    SetObjectiveDisplayed(80)
    StopSceneIfPlaying(AC_MQ02_Stage_PokerGame)
EndFunction

Function Fragment_Stage_1015_Item_00()
    SetObjectiveDisplayed(85)
EndFunction

Function Fragment_Stage_1016_Item_00()
    SetObjectiveCompleted(85)
    If AC_MQ02_Stage_StansLuckySlotsMessage != None
        AC_MQ02_Stage_StansLuckySlotsMessage.Show()
    EndIf
EndFunction

Function Fragment_Stage_1100_Item_00()
    SetObjectiveCompleted(80)
    SetObjectiveCompleted(85)
    SetObjectiveDisplayed(90)
EndFunction

Function Fragment_Stage_1200_Item_00()
    SetObjectiveCompleted(90)
    SetObjectiveDisplayed(100)
    EnableAlias(Alias_Actor_TicketClerk)
EndFunction

Function Fragment_Stage_1299_Item_00()
    If !IsStageDone(1300)
        SetStage(1300)
    EndIf
EndFunction

Function Fragment_Stage_1300_Item_00()
    SetObjectiveCompleted(100)
    SetObjectiveDisplayed(110)
    SetObjectiveDisplayed(111)
    SetObjectiveDisplayed(112)
    EnableAlias(Alias_EnableMarker_ClownDressingRoom)
    EnableAlias(Alias_Marker_ClownHat)
    EnableAlias(Alias_Marker_ClownOutfit)
    EnableAlias(Alias_Armor_ClownHat)
    EnableAlias(Alias_Armor_ClownOutfit)
EndFunction

Function Fragment_Stage_1375_Item_00()
    SetObjectiveDisplayed(111)
EndFunction

Function Fragment_Stage_1380_Item_00()
    SetObjectiveCompleted(111)
EndFunction

Function Fragment_Stage_1385_Item_00()
    SetObjectiveDisplayed(112)
EndFunction

Function Fragment_Stage_1390_Item_00()
    SetObjectiveCompleted(112)
EndFunction

Function Fragment_Stage_1400_Item_00()
    SetObjectiveCompleted(110)
    SetObjectiveCompleted(111)
    SetObjectiveCompleted(112)
    SetObjectiveDisplayed(115)
    EnableAlias(Alias_Door_VanityRoom)
EndFunction

Function Fragment_Stage_1450_Item_00()
    If QSTACMQ02WoodenDoorKnuckleKnock != None
        QSTACMQ02WoodenDoorKnuckleKnock.Play(Alias_Door_VanityRoom.GetReference())
    EndIf
EndFunction

Function Fragment_Stage_1500_Item_00()
    SetObjectiveCompleted(115)
    SetObjectiveDisplayed(120)
    EnableAlias(Alias_Actor_Zayde)
    StartSceneIfStopped(AC_MQ02_Stage_ZaydeMeeting)
EndFunction

Function Fragment_Stage_1599_Item_00()
    If !IsStageDone(1600)
        SetStage(1600)
    EndIf
EndFunction

Function Fragment_Stage_1600_Item_00()
    SetObjectiveCompleted(120)
    SetObjectiveDisplayed(130)
    EnableAlias(Alias_EnableMarker_StageProps)
    EnableAlias(Alias_EnableMarker_StageFrontProps)
    EnableAlias(Alias_EnableMarker_ShowProps)
    EnableAlias(Alias_EnableMarker_PerformanceClowns)
    EnableCollection(Alias_Actors_PerformanceClowns)
    EnableCollection(Alias_Actors_Audience)
    StartSceneIfStopped(AC_MQ02_Stage_ClownIntroduction)
    Quests:AC_MQ02_Stage:QuestScript controller = QuestController()
    If controller != None
        controller.BeginShowMusic()
    EndIf
EndFunction

Function Fragment_Stage_1650_Item_00()
    StartSceneIfStopped(AC_MQ02_Stage_AudienceReaction_Introduction)
EndFunction

Function Fragment_Stage_1730_Item_00()
    SetObjectiveCompleted(130)
    SetObjectiveDisplayed(135)
EndFunction

Function Fragment_Stage_1760_Item_00()
    PlayAudienceIdle(IdleClapping)
EndFunction

Function Fragment_Stage_1770_Item_00()
    StartSceneIfStopped(AC_MQ02_Stage_AudienceReaction_Positive)
    PlayAudienceIdle(IdleCheeringStanding)
EndFunction

Function Fragment_Stage_1780_Item_00()
    StartSceneIfStopped(AC_MQ02_Stage_AudienceReaction_Positive)
    PlayAudienceIdle(IdleClapping)
EndFunction

Function Fragment_Stage_1795_Item_00()
    StartSceneIfStopped(AC_MQ02_Stage_AudienceReaction_Negative)
    PlayAudienceIdle(IdleBooingStanding)
EndFunction

Function Fragment_Stage_1796_Item_00()
    StartSceneIfStopped(AC_MQ02_Stage_AudienceReaction_Negative)
    PlayAudienceIdle(IdleBooingStanding)
EndFunction

Function Fragment_Stage_1800_Item_00()
    SetObjectiveCompleted(135)
    SetObjectiveDisplayed(140)
EndFunction

Function Fragment_Stage_1830_Item_00()
    TryFinishDeadClowns()
EndFunction

Function Fragment_Stage_1840_Item_00()
    TryFinishDeadClowns()
EndFunction

Function Fragment_Stage_1850_Item_00()
    If !IsStageDone(1900)
        SetStage(1900)
    EndIf
EndFunction

Function Fragment_Stage_1900_Item_00()
    SetObjectiveCompleted(140)
    SetObjectiveDisplayed(150)
EndFunction

Function Fragment_Stage_2000_Item_00()
    SetObjectiveCompleted(150)
    SetObjectiveDisplayed(160)
EndFunction

Function Fragment_Stage_2050_Item_00()
    Quests:AC_MQ02_Stage:QuestScript controller = QuestController()
    If controller != None
        controller.BeginShowMusic()
    EndIf
EndFunction

Function Fragment_Stage_2100_Item_00()
    SetObjectiveCompleted(160)
    SetObjectiveDisplayed(170)
    EnableAlias(Alias_Actor_Evelyn_Pier)
    StartSceneIfStopped(AC_MQ02_Stage_EvelynBackstageApproach)
    Quests:AC_MQ02_Stage:QuestScript controller = QuestController()
    If controller != None
        controller.EndShowMusic()
    EndIf
EndFunction

Function Fragment_Stage_2105_Item_00()
    EnableAlias(Alias_Actor_Evelyn_Pier)
EndFunction

Function Fragment_Stage_2120_Item_00()
    Actor evelyn = ActorFromAlias(Alias_Actor_Evelyn_Pier)
    If evelyn != None
        evelyn.SetValue(AC_MQ02_Stage_EvelynOutlook_AV, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_2130_Item_00()
    Actor evelyn = ActorFromAlias(Alias_Actor_Evelyn_Pier)
    If evelyn != None
        evelyn.SetValue(AC_MQ02_Stage_EvelynOutlook_AV, -1.0)
    EndIf
EndFunction

Function Fragment_Stage_2200_Item_00()
    SetObjectiveCompleted(170)
    SetObjectiveDisplayed(175)
EndFunction

Function Fragment_Stage_2250_Item_00()
    SetObjectiveCompleted(175)
    SetObjectiveDisplayed(178)
    EnableAlias(Alias_Actor_Zayde)
    StartSceneIfStopped(AC_MQ02_Stage_KnifeThrowingIntro)
EndFunction

Function Fragment_Stage_2260_Item_00()
    SetObjectiveCompleted(178)
    SetObjectiveDisplayed(180)
    Quests:AC_MQ02_Stage:QuestScript controller = QuestController()
    If controller != None
        controller.PrepareKnifeThrowing()
    EndIf
EndFunction

Function Fragment_Stage_2299_Item_00()
    If !IsStageDone(2300)
        SetStage(2300)
    EndIf
EndFunction

Function Fragment_Stage_2300_Item_00()
    StartSceneIfStopped(AC_MQ02_Stage_AimedForBackboard)
EndFunction

Function Fragment_Stage_2310_Item_00()
    StartSceneIfStopped(AC_MQ02_Stage_AimedForTorso)
EndFunction

Function Fragment_Stage_2320_Item_00()
    StartSceneIfStopped(AC_MQ02_Stage_AimedForLimbs)
EndFunction

Function Fragment_Stage_2325_Item_00()
    StartSceneIfStopped(AC_MQ02_Stage_AimedForLimbs_01)
EndFunction

Function Fragment_Stage_2329_Item_00()
    If !IsStageDone(2330)
        SetStage(2330)
    EndIf
EndFunction

Function Fragment_Stage_2330_Item_00()
    Actor zayde = ActorFromAlias(Alias_Actor_Zayde)
    If zayde != None && !zayde.IsDead()
        zayde.Kill(ActorFromAlias(Alias_Player))
    EndIf
    StartSceneIfStopped(AC_MQ02_Stage_AudienceReaction_ZaydeKilled)
EndFunction

Function Fragment_Stage_2400_Item_00()
    SetObjectiveCompleted(180)
    SetObjectiveDisplayed(200)
    SetObjectiveDisplayed(205)
    Quests:AC_MQ02_Stage:QuestScript controller = QuestController()
    If controller != None
        controller.EndShowMusic()
    EndIf
EndFunction

Function Fragment_Stage_2500_Item_00()
    EnableAlias(Alias_Actor_StarClown)
    StartSceneIfStopped(AC_MQ02_Stage_StarClownEntrance)
EndFunction

Function Fragment_Stage_2550_Item_00()
    Actor starClown = ActorFromAlias(Alias_Actor_StarClown)
    Actor player = ActorFromAlias(Alias_Player)
    If starClown != None && player != None
        starClown.StartCombat(player)
    EndIf
EndFunction

Function Fragment_Stage_2600_Item_00()
    SetObjectiveCompleted(205)
EndFunction

Function Fragment_Stage_2700_Item_00()
    SetObjectiveCompleted(200)
    SetObjectiveCompleted(205)
    SetObjectiveDisplayed(210)
    EnableAlias(Alias_Actor_Stan_Boardwalk)
    StartSceneIfStopped(AC_MQ02_Stage_StanToCasinoQuarterScene)
EndFunction

Function Fragment_Stage_2800_Item_00()
    SetObjectiveCompleted(210)
    SetObjectiveDisplayed(215)
    GiveDevilsBloodIfMissing()
EndFunction

Function Fragment_Stage_2850_Item_00()
    SetObjectiveCompleted(215)
    SetObjectiveDisplayed(220)
    StartSceneIfStopped(AC_MQ02_Stage_EvelynAbbieAmbient)
EndFunction

Function Fragment_Stage_2899_Item_00()
    If !IsStageDone(2900)
        SetStage(2900)
    EndIf
EndFunction

Function Fragment_Stage_2900_Item_00()
    SetObjectiveCompleted(220)
    SetObjectiveDisplayed(230)
    RemoveDevilsBlood()
EndFunction

Function Fragment_Stage_9000_Item_00()
    SetObjectiveCompleted(230)
    ObjectReference player = PlayerReference()
    If AC_MQ03_HonorBound_StartKeyword != None && player != None && AC_MQ03_HonorBound != None && !AC_MQ03_HonorBound.IsRunning() && !AC_MQ03_HonorBound.IsCompleted()
        AC_MQ03_HonorBound_StartKeyword.SendStoryEvent(None, player, player)
    EndIf
    If !IsStageDone(9100)
        SetStage(9100)
    EndIf
EndFunction

Function Fragment_Stage_9100_Item_00()
    StopSceneIfPlaying(AC_MQ02_Stage_EvelynAbbieAmbient)
    StopSceneIfPlaying(AC_MQ02_Stage_EvelynBackstageApproach)
    StopSceneIfPlaying(AC_MQ02_Stage_KnifeThrowingIntro)
    StopSceneIfPlaying(AC_MQ02_Stage_StarClownEntrance)
    Quests:AC_MQ02_Stage:QuestScript controller = QuestController()
    If controller != None
        controller.ShutdownLocalShow()
    EndIf
    Stop()
EndFunction
