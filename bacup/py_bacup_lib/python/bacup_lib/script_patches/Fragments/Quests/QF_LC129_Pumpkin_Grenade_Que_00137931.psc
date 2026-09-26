Function Fragment_Stage_0001_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && Alias_QuestPlayer.GetReference() != playerRef
        Alias_QuestPlayer.ForceRefTo(playerRef)
    EndIf
    WatchQuestGiver()
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0100_Item_00()
    WatchQuestGiver()
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0200_Item_00()
    WatchQuestGiver()
    If WelcomeScene != None && !WelcomeScene.IsPlaying()
        WelcomeScene.Start()
    EndIf

    ; Jack's welcome scene carries no playable converted dialogue, so it either
    ; refuses to start or ends without ever setting stage 300. Bound the wait and
    ; hand the quest over as soon as the scene is no longer running.
    Int waited = 0
    While WelcomeScene != None && WelcomeScene.IsPlaying() && waited < 20
        Utility.Wait(0.5)
        waited += 1
    EndWhile

    If !IsStageDone(300)
        SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(20)

    WatchQuestGiver()
    StartTimer(3.0, 1)

    DefaultAliasInventoryManagement inventoryManager = Alias_QuestPlayer as DefaultAliasInventoryManagement
    If inventoryManager != None
        inventoryManager.EvaluateInventoryState()
    EndIf
    RefreshDeliveryObjective()
EndFunction

Function Fragment_Stage_0600_Item_00()
    Actor playerRef = Alias_QuestPlayer.GetActorReference()
    If playerRef != None
        Int pumpkinsToRemove = playerRef.GetItemCount(PumpkinItem)
        If pumpkinsToRemove > 10
            pumpkinsToRemove = 10
        EndIf
        If pumpkinsToRemove > 0
            playerRef.RemoveItem(PumpkinItem, pumpkinsToRemove, true)
        EndIf
        If LC129_DailyCompletedFirstTime != None && playerRef.GetValue(LC129_DailyCompletedFirstTime) <= 0.0
            playerRef.SetValue(LC129_DailyCompletedFirstTime, 1.0)
        EndIf
    EndIf

    CancelTimer(1)
    SetObjectiveCompleted(20)
    SetObjectiveCompleted(30)

    ; Stage 600 carries a second log entry that the conversion appended to pay
    ; the currency reward (B21:CurrencyQuestRewards RewardStageItems = 1). Yield
    ; before stage 1000 stops the quest so that entry is processed first.
    Utility.Wait(1.0)
    If !IsStageDone(1000)
        SetStage(1000)
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    CompleteAllObjectives()
    If GoodbyeScene != None && GoodbyeScene.IsPlaying()
        GoodbyeScene.Stop()
    EndIf
    StopWatching()
    Stop()
EndFunction

Function WatchQuestGiver()
    ObjectReference giverRef = Alias_QuestGiver.GetReference()
    If giverRef != None
        RegisterForRemoteEvent(giverRef, "OnActivate")
    EndIf
EndFunction

Function StopWatching()
    CancelTimer(1)
    ObjectReference giverRef = Alias_QuestGiver.GetReference()
    If giverRef != None
        UnregisterForRemoteEvent(giverRef, "OnActivate")
    EndIf
    Actor playerRef = Alias_QuestPlayer.GetActorReference()
    If playerRef != None && giverRef != None
        UnregisterForDistanceEvents(playerRef, giverRef)
    EndIf
EndFunction

; The pumpkin count is polled rather than driven by inventory events: this is a
; Quest script, so it cannot own an inventory filter on the player, and the
; player's own filter belongs to DefaultAliasInventoryManagement.
Event OnTimer(Int aiTimerID)
    If aiTimerID != 1
        Return
    EndIf
    RefreshDeliveryObjective()
    If IsRunning() && !IsStageDone(600)
        StartTimer(3.0, 1)
    EndIf
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActivator)
    If akActivator == Game.GetPlayer() && akSender == Alias_QuestGiver.GetReference()
        ReachQuestGiver()
    EndIf
EndEvent

Event OnDistanceLessThan(ObjectReference akObj1, ObjectReference akObj2, Float afDistance)
    ReachQuestGiver()
EndEvent

; Both of Jack's scenes lost their dialogue in conversion, so reaching or
; activating him substitutes for the conversation that used to move these stages.
Function ReachQuestGiver()
    If !IsRunning() || IsStageDone(600)
        Return
    EndIf
    If !IsStageDone(300)
        SetStage(300)
        Return
    EndIf
    If HasDeliveryCount()
        Quest owner = Self as Quest
        Quests:LC129:PumpkinQuestScript pumpkinQuest = owner as Quests:LC129:PumpkinQuestScript
        If pumpkinQuest != None
            pumpkinQuest.TryTurnInPumpkins()
        EndIf
    EndIf
EndFunction

; The quest's DefaultAliasInventoryManagement row sets stage 400, which this
; quest never defines, so the shared script returns before it can complete
; objective 20 or display objective 30. Own that pair here instead.
Function RefreshDeliveryObjective()
    If !IsRunning() || !IsStageDone(300) || IsStageDone(600)
        Return
    EndIf

    ObjectReference giverRef = Alias_QuestGiver.GetReference()
    Actor playerRef = Alias_QuestPlayer.GetActorReference()
    If HasDeliveryCount()
        If !IsObjectiveDisplayed(30)
            SetObjectiveCompleted(20)
            SetObjectiveDisplayed(30)
            If giverRef != None && playerRef != None
                RegisterForDistanceLessThanEvent(playerRef, giverRef, 250.0)
            EndIf
        EndIf
    ElseIf IsObjectiveDisplayed(30)
        SetObjectiveDisplayed(30, false)
        SetObjectiveCompleted(20, false)
        SetObjectiveDisplayed(20)
        If giverRef != None && playerRef != None
            UnregisterForDistanceEvents(playerRef, giverRef)
        EndIf
    EndIf
EndFunction

Bool Function HasDeliveryCount()
    Actor playerRef = Alias_QuestPlayer.GetActorReference()
    If playerRef == None || PumpkinItem == None
        Return false
    EndIf
    Return playerRef.GetItemCount(PumpkinItem) >= 10
EndFunction
