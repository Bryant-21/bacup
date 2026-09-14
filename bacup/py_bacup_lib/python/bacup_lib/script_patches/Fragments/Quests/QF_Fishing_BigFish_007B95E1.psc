Function Fragment_Stage_0100_Item_00()
    Actor playerRef = Game.GetPlayer()
    Alias_Player.ForceRefIfEmpty(playerRef)
    playerRef.SetValue(FishCaughtAV, 0.0)
    playerRef.SetValue(chosenRegionAV, -1.0)
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
    SetObjectiveDisplayed(25)
EndFunction

Function Fragment_Stage_0252_Item_00()
    Alias_Player.GetActorReference().SetValue(chosenRegionAV, 1.0)
    SetObjectiveDisplayed(26)
EndFunction

Function Fragment_Stage_0253_Item_00()
    Alias_Player.GetActorReference().SetValue(chosenRegionAV, 2.0)
    SetObjectiveDisplayed(27)
EndFunction

Function Fragment_Stage_0254_Item_00()
    Alias_Player.GetActorReference().SetValue(chosenRegionAV, 3.0)
    SetObjectiveDisplayed(28)
EndFunction

Function Fragment_Stage_0255_Item_00()
    Alias_Player.GetActorReference().SetValue(chosenRegionAV, 4.0)
    SetObjectiveDisplayed(29)
EndFunction

Function Fragment_Stage_0256_Item_00()
    Alias_Player.GetActorReference().SetValue(chosenRegionAV, 5.0)
    SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_0257_Item_00()
    Alias_Player.GetActorReference().SetValue(chosenRegionAV, 6.0)
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
        playerRef.SetValue(CompletionTracker_AV, 1.0)
        playerRef.SetValue(FishCaughtAV, 0.0)
    EndIf
    CompleteQuest()
    Stop()
EndFunction
