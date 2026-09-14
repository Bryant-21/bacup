Function Fragment_Stage_0010_Item_00()
    Quest owningQuest = GetOwningQuest()
    owningQuest.SetObjectiveCompleted(300)
    D01C_StingsAndThingsScript stingsQuest = D01C_StingsAndThings as D01C_StingsAndThingsScript
    If stingsQuest != None
        stingsQuest.CheckAllPartsCollected()
    EndIf
EndFunction

Function Fragment_Stage_0020_Item_00()
    Quest owningQuest = GetOwningQuest()
    owningQuest.SetObjectiveCompleted(400)
    D01C_StingsAndThingsScript stingsQuest = D01C_StingsAndThings as D01C_StingsAndThingsScript
    If stingsQuest != None
        stingsQuest.CheckAllPartsCollected()
    EndIf
EndFunction

Function Fragment_Stage_0030_Item_00()
    Quest owningQuest = GetOwningQuest()
    owningQuest.SetObjectiveCompleted(500)
    D01C_StingsAndThingsScript stingsQuest = D01C_StingsAndThings as D01C_StingsAndThingsScript
    If stingsQuest != None
        stingsQuest.CheckAllPartsCollected()
    EndIf
EndFunction

Function Fragment_Stage_0040_Item_00()
    Quest owningQuest = GetOwningQuest()
    owningQuest.SetObjectiveCompleted(600)
    D01C_StingsAndThingsScript stingsQuest = D01C_StingsAndThings as D01C_StingsAndThingsScript
    If stingsQuest != None
        stingsQuest.CheckAllPartsCollected()
    EndIf
EndFunction

Function Fragment_Stage_0050_Item_00()
    Quest owningQuest = GetOwningQuest()
    owningQuest.SetObjectiveCompleted(700)
    D01C_StingsAndThingsScript stingsQuest = D01C_StingsAndThings as D01C_StingsAndThingsScript
    If stingsQuest != None
        stingsQuest.CheckAllPartsCollected()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    GetOwningQuest().SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0200_Item_00()
    Quest owningQuest = GetOwningQuest()
    owningQuest.SetObjectiveCompleted(100)
    owningQuest.SetObjectiveDisplayed(300)
    owningQuest.SetObjectiveDisplayed(400)
    owningQuest.SetObjectiveDisplayed(500)
    owningQuest.SetObjectiveDisplayed(600)
    owningQuest.SetObjectiveDisplayed(700)

    If D01C_StingsAndThings_Intro != None && !D01C_StingsAndThings_Intro.IsPlaying()
        D01C_StingsAndThings_Intro.Start()
    EndIf

    D01C_StingsAndThingsScript stingsQuest = D01C_StingsAndThings as D01C_StingsAndThingsScript
    If stingsQuest != None
        stingsQuest.CheckExistingParts()
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    Quest owningQuest = GetOwningQuest()
    owningQuest.SetObjectiveCompleted(300)
    owningQuest.SetObjectiveCompleted(400)
    owningQuest.SetObjectiveCompleted(500)
    owningQuest.SetObjectiveCompleted(600)
    owningQuest.SetObjectiveCompleted(700)
    owningQuest.SetObjectiveDisplayed(800)
EndFunction

Function Fragment_Stage_0900_Item_00()
EndFunction

Function Fragment_Stage_1000_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None
        playerRef.RemoveItem(BloatflyGland, 1, true)
        playerRef.RemoveItem(BloodbugProboscis, 1, true)
        playerRef.RemoveItem(RadroachMeat, 1, true)
        playerRef.RemoveItem(StingwingBarb, 1, true)
        playerRef.RemoveItem(TickBloodSac, 1, true)
        Potion insectRepellent = Game.GetFormFromFile(0x0045E3F6, "SeventySix.esm") as Potion
        If insectRepellent != None
            playerRef.AddItem(insectRepellent, 1, true)
        EndIf
        If D01C_Stings_DailyTimestamp != None
            playerRef.SetValue(D01C_Stings_DailyTimestamp, Utility.GetCurrentGameTime())
        EndIf
    EndIf

    Quest owningQuest = GetOwningQuest()
    owningQuest.SetObjectiveCompleted(800)

    P01C_TadpoleQuest tadpole = TadpoleQuest as P01C_TadpoleQuest
    If tadpole != None
        tadpole.OnActivityCompleted(1)
    EndIf
EndFunction

Function Fragment_Stage_10000_Item_00()
EndFunction
