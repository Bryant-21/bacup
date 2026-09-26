Function Fragment_Stage_0100_Item_00()
    Actor playerRef = Game.GetPlayer()
    Alias_Player.ForceRefIfEmpty(playerRef)
    playerRef.SetValue(FishCaughtAV, 0.0)
    playerRef.SetValue(chosenRegionAV, -1.0)
    playerRef.SetValue(hasRegionAV, 0.0)
    SetObjectiveDisplayed(10)
    If CaptainCalloutScene != None && !CaptainCalloutScene.IsPlaying()
        CaptainCalloutScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(10)
EndFunction

Function Fragment_Stage_0251_Item_00()
    Alias_Player.GetActorReference().SetValue(chosenRegionAV, 0.0)
    Alias_Player.GetActorReference().SetValue(hasRegionAV, 1.0)
    SetObjectiveDisplayed(25)
EndFunction

Function Fragment_Stage_0252_Item_00()
    Alias_Player.GetActorReference().SetValue(chosenRegionAV, 1.0)
    Alias_Player.GetActorReference().SetValue(hasRegionAV, 1.0)
    SetObjectiveDisplayed(26)
EndFunction

Function Fragment_Stage_0253_Item_00()
    Alias_Player.GetActorReference().SetValue(chosenRegionAV, 2.0)
    Alias_Player.GetActorReference().SetValue(hasRegionAV, 1.0)
    SetObjectiveDisplayed(27)
EndFunction

Function Fragment_Stage_0254_Item_00()
    Alias_Player.GetActorReference().SetValue(chosenRegionAV, 3.0)
    Alias_Player.GetActorReference().SetValue(hasRegionAV, 1.0)
    SetObjectiveDisplayed(28)
EndFunction

Function Fragment_Stage_0255_Item_00()
    Alias_Player.GetActorReference().SetValue(chosenRegionAV, 4.0)
    Alias_Player.GetActorReference().SetValue(hasRegionAV, 1.0)
    SetObjectiveDisplayed(29)
EndFunction

Function Fragment_Stage_0256_Item_00()
    Alias_Player.GetActorReference().SetValue(chosenRegionAV, 5.0)
    Alias_Player.GetActorReference().SetValue(hasRegionAV, 1.0)
    SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_0257_Item_00()
    Alias_Player.GetActorReference().SetValue(chosenRegionAV, 6.0)
    Alias_Player.GetActorReference().SetValue(hasRegionAV, 1.0)
    SetObjectiveDisplayed(31)
EndFunction

Function Fragment_Stage_0270_Item_00()
    Int objectiveIndex = 25
    While objectiveIndex <= 31
        If IsObjectiveDisplayed(objectiveIndex)
            SetObjectiveCompleted(objectiveIndex)
        EndIf
        objectiveIndex += 1
    EndWhile
    SetObjectiveDisplayed(40)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(40)
EndFunction

Function Fragment_Stage_9000_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    SetObjectiveCompleted(40)
    If playerRef != None
        Form completionItems = Game.GetFormFromFile(0x007DC663, "SeventySix.esm")
        If completionItems != None
            playerRef.AddItem(completionItems, 1)
        EndIf
        Int caughtRegionIndex = playerRef.GetValue(chosenRegionAV) as Int
        Int rewardID = 0
        If caughtRegionIndex == 0
            rewardID = 0x0031355B
        ElseIf caughtRegionIndex == 1
            rewardID = 0x0031355C
        ElseIf caughtRegionIndex == 2
            rewardID = 0x00313574
        ElseIf caughtRegionIndex == 3
            rewardID = 0x00313562
        ElseIf caughtRegionIndex == 4 || caughtRegionIndex == 6
            rewardID = 0x00313561
        ElseIf caughtRegionIndex == 5
            rewardID = 0x00313558
        EndIf
        If rewardID != 0
            Form regionReward = Game.GetFormFromFile(rewardID, "SeventySix.esm")
            If regionReward != None
                playerRef.AddItem(regionReward, 1)
            EndIf
        EndIf
        playerRef.SetValue(CompletionTracker_AV, 1.0)
        playerRef.SetValue(FishCaughtAV, 0.0)
        playerRef.SetValue(hasRegionAV, 0.0)
    EndIf
    CompleteQuest()
    Stop()
EndFunction

Function Fishing_RecordCatch(Int aiItemCount, Int aiRegionMask)
    If !IsRunning()
        Return
    EndIf
    Fishing_BigFish_FishingScript counter = Alias_Player as Fishing_BigFish_FishingScript
    If counter != None
        counter.OnFishingCatch(aiItemCount, aiRegionMask)
    EndIf
EndFunction
