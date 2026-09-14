Function Fragment_Stage_0500_Item_00()
    Quest owningQuest = GetOwningQuest()
    owningQuest.SetObjectiveCompleted(30)
    owningQuest.SetObjectiveDisplayed(40)
    owningQuest.SetObjectiveDisplayed(50)
    owningQuest.SetObjectiveDisplayed(60)
    owningQuest.SetObjectiveDisplayed(70)
    owningQuest.SetObjectiveDisplayed(80)

    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef == None
        Return
    EndIf

    If ScoutUniform != None && playerRef.GetItemCount(ScoutUniform) == 0
        playerRef.AddItem(ScoutUniform, 1, false)
    EndIf
    If FrogJar != None && playerRef.GetItemCount(FrogJar) == 0
        playerRef.AddItem(FrogJar, 1, true)
    EndIf

    If OpTidyQuest != None && !OpTidyQuest.IsRunning() && !OpTidyQuest.IsCompleted()
        TidyQuestKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
    EndIf
    If StingsAndThingsQuest != None && !StingsAndThingsQuest.IsRunning() && !StingsAndThingsQuest.IsCompleted()
        StingsQuestKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(30)
    If JaggyWelcome != None && !JaggyWelcome.IsPlaying()
        JaggyWelcome.Start()
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveCompleted(50)
EndFunction

Function Fragment_Stage_1100_Item_00()
    SetObjectiveCompleted(80)
EndFunction

Function Fragment_Stage_1200_Item_00()
    SetObjectiveCompleted(60)
EndFunction

Function Fragment_Stage_1300_Item_00()
    SetObjectiveCompleted(70)
EndFunction

Function Fragment_Stage_2000_Item_00()
    SetObjectiveCompleted(40)
    SetObjectiveDisplayed(90)
EndFunction

Function Fragment_Stage_2100_Item_00()
    If JaggyPromotion != None && !JaggyPromotion.IsPlaying()
        JaggyPromotion.Start()
    EndIf
EndFunction

Function Fragment_Stage_2200_Item_00()
    If PompyPromotion != None && !PompyPromotion.IsPlaying()
        PompyPromotion.Start()
    EndIf
EndFunction

Function Fragment_Stage_2300_Item_00()
    If TreadlyPromotion != None && !TreadlyPromotion.IsPlaying()
        TreadlyPromotion.Start()
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    SetObjectiveCompleted(90)
EndFunction
