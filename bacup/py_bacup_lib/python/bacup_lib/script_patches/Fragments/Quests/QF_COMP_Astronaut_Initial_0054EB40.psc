Function Fragment_Stage_0100_Item_00()
    SetStageOnce(200)
EndFunction

Function Fragment_Stage_0200_Item_00()
    DisplayObjectiveOnce(100)
EndFunction

Function Fragment_Stage_0225_Item_00()
    CompleteObjectiveOnce(100)
    Actor player = GetPlayerActor()
    If player && HasAnyStimpak(player)
        SetStageOnce(230)
    Else
        SetStageOnce(224)
        DisplayObjectiveOnce(225)
    EndIf
EndFunction

Function Fragment_Stage_0226_Item_00()
    ObjectReference medStash = Alias_MedStash.GetReference()
    If medStash && Stimpak && medStash.GetItemCount(Stimpak) <= 0
        SetStageOnce(227)
    EndIf
EndFunction

Function Fragment_Stage_0230_Item_00()
    CompleteObjectiveOnce(225)
    DisplayObjectiveOnce(230)
EndFunction

Function Fragment_Stage_0300_Item_00()
    CompleteObjectiveOnce(100)
EndFunction

Function Fragment_Stage_0305_Item_00()
    Actor player = GetPlayerActor()
    If player
        RemoveOneStimpak(player)
    EndIf
    CompleteObjectiveOnce(100)
    CompleteObjectiveOnce(225)
    CompleteObjectiveOnce(230)
    DisplayObjectiveOnce(305)
EndFunction

Function Fragment_Stage_1000_Item_00()
    CompleteObjectiveOnce(305)
    DisplayObjectiveOnce(1000)
EndFunction

Function Fragment_Stage_1190_Item_00()
    CompleteObjectiveOnce(1000)
    CompleteObjectiveOnce(1100)
    DisplayObjectiveOnce(1190)
    If ALLY_Astronaut_Scene_Intro_RecorderScene && !ALLY_Astronaut_Scene_Intro_RecorderScene.IsPlaying()
        ALLY_Astronaut_Scene_Intro_RecorderScene.Start()
    EndIf
    ; FO76 ran the flight recorder download on a hollowed timer; nothing else sets 1191,
    ; and the collect activator (DefaultAliasOnActivateC) requires it. Two minutes per the
    ; quest's documented download countdown.
    StartTimer(120.0, 1191)
    If COMP_Keyword_QuestStart_Astronaut_Intro_SpawnQuest != None
        COMP_Keyword_QuestStart_Astronaut_Intro_SpawnQuest.SendStoryEventAndWait()
    EndIf
EndFunction

Function Fragment_Stage_1191_Item_00()
    CancelTimer(1191)
    CompleteObjectiveOnce(1190)
    DisplayObjectiveOnce(1195)
EndFunction

Function Fragment_Stage_1192_Item_00()
    If !IsStageDone(1191) && ALLY_Astronaut_Scene_Intro_RecorderProcessingScene && !ALLY_Astronaut_Scene_Intro_RecorderProcessingScene.IsPlaying()
        ALLY_Astronaut_Scene_Intro_RecorderProcessingScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1195_Item_00()
    If COMP_Quest_Intro_Astronaut_CrashSpawnQuest != None && COMP_Quest_Intro_Astronaut_CrashSpawnQuest.IsRunning()
        COMP_Quest_Intro_Astronaut_CrashSpawnQuest.SetStage(9000)
    EndIf
    Actor player = GetPlayerActor()
    ObjectReference flightRecorder = Alias_FlightRecorder.GetReference()
    If player && flightRecorder && flightRecorder.GetContainer() != player
        player.AddItem(flightRecorder, 1)
    EndIf
    If player && COMP_AV_Astronaut_Intro_PlayerCollectedFlightRecorder
        player.SetValue(COMP_AV_Astronaut_Intro_PlayerCollectedFlightRecorder, 1.0)
    EndIf
    CompleteObjectiveOnce(1195)
    SetStageOnce(1200)
EndFunction

Function Fragment_Stage_1200_Item_00()
    DisplayObjectiveOnce(1200)
EndFunction

Function Fragment_Stage_1290_Item_00()
    CompleteObjectiveOnce(1200)
EndFunction

Function Fragment_Stage_1300_Item_00()
    CompleteObjectiveOnce(1200)
    DisplayObjectiveOnce(1300)
EndFunction

Function Fragment_Stage_1310_Item_00()
    Actor player = GetPlayerActor()
    If player && AV_PlayerKnows_BlueSunset
        player.SetValue(AV_PlayerKnows_BlueSunset, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_1330_Item_00()
    ; The Charisma payment line answers "Here." - the documented payout is 50 pre-War money.
    Actor player = GetPlayerActor()
    If player && Money
        player.AddItem(Money, 50)
    EndIf
EndFunction

Function Fragment_Stage_1350_Item_00()
    Actor player = GetPlayerActor()
    Actor robot = Alias_USSABot.GetActorReference()
    If !robot || robot.IsDead()
        Return
    EndIf
    If Alias_Actor_Assaultron_Invulnerable.GetActorReference() == robot
        Alias_Actor_Assaultron_Invulnerable.Clear()
    EndIf
    If COMP_Astronaut_Intro_AssaultronEnemyFaction
        robot.AddToFaction(COMP_Astronaut_Intro_AssaultronEnemyFaction)
    EndIf
    If player
        robot.StartCombat(player)
    EndIf
EndFunction

Function Fragment_Stage_1380_Item_00()
    SetStageOnce(2000)
EndFunction

Function Fragment_Stage_1385_Item_00()
    Actor player = GetPlayerActor()
    If player && EncryptionKey && player.GetItemCount(EncryptionKey) <= 0
        Actor robot = Alias_USSABot.GetActorReference()
        If robot && robot.GetItemCount(EncryptionKey) > 0
            robot.RemoveItem(EncryptionKey, 1, True, player)
        Else
            player.AddItem(EncryptionKey, 1)
        EndIf
    EndIf
    SetStageOnce(2000)
EndFunction

Function Fragment_Stage_1390_Item_00()
    SetStageOnce(1385)
EndFunction

Function Fragment_Stage_2000_Item_00()
    CompleteObjectiveOnce(1300)
    DisplayObjectiveOnce(2001)
EndFunction

Function Fragment_Stage_3000_Item_00()
    CompleteObjectiveOnce(2000)
    CompleteObjectiveOnce(2001)
    DisplayObjectiveOnce(3000)
EndFunction

Function Fragment_Stage_3500_Item_00()
    CompleteObjectiveOnce(3000)
    SetStageOnce(9000)
EndFunction

Function Fragment_Stage_9000_Item_00()
    CompleteQuest()
    Stop()
EndFunction

Function Fragment_Stage_9990_Item_00()
    FailAllObjectives()
    Stop()
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == 1191 && IsRunning() && IsStageDone(1190)
        SetStageOnce(1191)
    EndIf
EndEvent

Actor Function GetPlayerActor()
    Actor player = Alias_Player.GetActorReference()
    If !player
        player = Game.GetPlayer()
        If player
            Alias_Player.ForceRefIfEmpty(player)
        EndIf
    EndIf
    Return player
EndFunction

Function SetStageOnce(Int stage)
    If !IsStageDone(stage)
        SetStage(stage)
    EndIf
EndFunction

Function DisplayObjectiveOnce(Int objective)
    If !IsObjectiveDisplayed(objective) && !IsObjectiveCompleted(objective) && !IsObjectiveFailed(objective)
        SetObjectiveDisplayed(objective)
    EndIf
EndFunction

Function CompleteObjectiveOnce(Int objective)
    If IsObjectiveDisplayed(objective) && !IsObjectiveCompleted(objective) && !IsObjectiveFailed(objective)
        SetObjectiveCompleted(objective)
    EndIf
EndFunction

Bool Function HasAnyStimpak(Actor player)
    Return (Stimpak && player.GetItemCount(Stimpak) > 0) || (DilutedStimpak && player.GetItemCount(DilutedStimpak) > 0) || (SuperStimpak && player.GetItemCount(SuperStimpak) > 0)
EndFunction

Function RemoveOneStimpak(Actor player)
    If Stimpak && player.GetItemCount(Stimpak) > 0
        player.RemoveItem(Stimpak, 1, True)
    ElseIf DilutedStimpak && player.GetItemCount(DilutedStimpak) > 0
        player.RemoveItem(DilutedStimpak, 1, True)
    ElseIf SuperStimpak && player.GetItemCount(SuperStimpak) > 0
        player.RemoveItem(SuperStimpak, 1, True)
    EndIf
EndFunction
