Function Fragment_Stage_0100_Item_00()
    Actor playerRef = Game.GetPlayer()
    Alias_Player.ForceRefIfEmpty(playerRef)
    playerRef.SetValue(Fishing_MQ01_FishCaught, 0.0)
    playerRef.SetValue(Fishing_ChumTrough_AV, 0.0)
    SetObjectiveDisplayed(5)

    If Fishing_MQ01_Casting_Radio != None && !Fishing_MQ01_Casting_Radio.IsRunning()
        Fishing_MQ01_Casting_Radio_StarKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
    EndIf
EndFunction

Function Fragment_Stage_0150_Item_00()
    SetObjectiveCompleted(5)
    SetObjectiveDisplayed(10)
    If Fishing_MQ01_Casting_Radio != None && Fishing_MQ01_Casting_Radio.IsRunning()
        Fishing_MQ01_Casting_Radio.Stop()
    EndIf
EndFunction

Function Fragment_Stage_0151_Item_00()
    SetObjectiveCompleted(5)
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(30)
    SetObjectiveDisplayed(40)
EndFunction

Function Fragment_Stage_0500_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    SetObjectiveCompleted(40)
    If playerRef != None
        If FishingRod != None && playerRef.GetItemCount(FishingRod) == 0
            playerRef.AddItem(FishingRod, 1)
            If RodReceiveSound != None
                RodReceiveSound.Play(playerRef)
            EndIf
        EndIf
        If DefaultBait != None && playerRef.GetItemCount(DefaultBait) < 3
            playerRef.AddItem(DefaultBait, 3 - playerRef.GetItemCount(DefaultBait))
        EndIf
        playerRef.SetValue(Fishing_MQ01_FishCaught, 0.0)
    EndIf
    SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(50)
    SetObjectiveDisplayed(60)
EndFunction

Function Fragment_Stage_0700_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    Form fishBits = Game.GetFormFromFile(0x007CE310, "SeventySix.esm")
    Int requiredFishBits = 3
    If Fishing_ChumTrough_RewardThreshold != None && Fishing_ChumTrough_RewardThreshold.GetValue() > requiredFishBits
        requiredFishBits = Fishing_ChumTrough_RewardThreshold.GetValue() as Int
    EndIf
    SetObjectiveCompleted(60)
    SetObjectiveDisplayed(70)

    If playerRef != None && fishBits != None && playerRef.GetItemCount(fishBits) < requiredFishBits
        playerRef.AddItem(fishBits, requiredFishBits - playerRef.GetItemCount(fishBits))
    EndIf

    If !IsStageDone(800)
        SetStage(800)
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveCompleted(70)
    SetObjectiveDisplayed(80)
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveCompleted(80)
    SetObjectiveDisplayed(90)
EndFunction

Function Fragment_Stage_0950_Item_00()
    If Fishing_MQ01_FishingEnabledMessage != None
        Fishing_MQ01_FishingEnabledMessage.Show()
    EndIf
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    SetObjectiveCompleted(90)
    CompleteQuest()

    If Fishing_TryStartBigFish()
        Stop()
    Else
        StartTimer(5.0, 9000)
    EndIf
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID != 9000 || !IsStageDone(9000)
        Return
    EndIf

    If Fishing_TryStartBigFish()
        Stop()
    Else
        StartTimer(5.0, 9000)
    EndIf
EndEvent

Bool Function Fishing_TryStartBigFish()
    Quest bigFishQuest = Game.GetFormFromFile(0x007B95E1, "SeventySix.esm") as Quest
    If bigFishQuest != None && (bigFishQuest.IsRunning() || bigFishQuest.IsCompleted())
        Return True
    EndIf
    If Fishing_BigFish_StartKeyword == None
        Return False
    EndIf

    Actor playerRef = Alias_Player.GetActorReference()
    Bool accepted = Fishing_BigFish_StartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
    Return accepted || (bigFishQuest != None && (bigFishQuest.IsRunning() || bigFishQuest.IsCompleted()))
EndFunction
