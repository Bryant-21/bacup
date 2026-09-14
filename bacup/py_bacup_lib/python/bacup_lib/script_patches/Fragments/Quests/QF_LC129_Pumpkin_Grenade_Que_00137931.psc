Function Fragment_Stage_0001_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && Alias_QuestPlayer.GetReference() != playerRef
        Alias_QuestPlayer.ForceRefTo(playerRef)
    EndIf
    GetOwningQuest().SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0100_Item_00()
    GetOwningQuest().SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0200_Item_00()
    If WelcomeScene != None && !WelcomeScene.IsPlaying()
        WelcomeScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    Quest owningQuest = GetOwningQuest()
    owningQuest.SetObjectiveCompleted(10)
    owningQuest.SetObjectiveDisplayed(20)

    DefaultAliasInventoryManagement inventoryManager = Alias_QuestPlayer as DefaultAliasInventoryManagement
    If inventoryManager != None
        inventoryManager.EvaluateInventoryState()
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    Quest owningQuest = GetOwningQuest()
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

    owningQuest.SetObjectiveCompleted(20)
    owningQuest.SetObjectiveCompleted(30)
EndFunction

Function Fragment_Stage_1000_Item_00()
    Quest owningQuest = GetOwningQuest()
    owningQuest.CompleteAllObjectives()
    If GoodbyeScene != None && GoodbyeScene.IsPlaying()
        GoodbyeScene.Stop()
    EndIf
    owningQuest.Stop()
EndFunction
