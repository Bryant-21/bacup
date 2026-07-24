; TODO

Function Fragment_Stage_0010_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef && RS01A_Contact_Started
        playerRef.SetValue(RS01A_Contact_Started, 1.0)
    EndIf
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0150_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        If MQ_Overseer_01_Vault76Holotape && playerRef.GetItemCount(MQ_Overseer_01_Vault76Holotape) == 0
            playerRef.AddItem(MQ_Overseer_01_Vault76Holotape, 1, False)
        EndIf
        If MQ_OverseerHolotape01PickedUp
            playerRef.SetValue(MQ_OverseerHolotape01PickedUp, 1.0)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0160_Item_00()
    SetObjectiveCompleted(100)
    SetObjectiveDisplayed(110)
EndFunction

Function Fragment_Stage_0165_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        If Tutorial_PlaceCAMPStartKeyword
            Tutorial_PlaceCAMPStartKeyword.SendStoryEvent(None, playerRef, playerRef)
        EndIf
        If Tutorial_WeaponCraftingStartKeyword
            Tutorial_WeaponCraftingStartKeyword.SendStoryEvent(None, playerRef, playerRef)
        EndIf
        If Tutorial_ArmorCraftingStartKeyword
            Tutorial_ArmorCraftingStartKeyword.SendStoryEvent(None, playerRef, playerRef)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0170_Item_00()
    SetObjectiveCompleted(110)
    SetObjectiveDisplayed(120)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(120)
    SetObjectiveDisplayed(200)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(200)
    SetObjectiveDisplayed(400)
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(400)
    SetObjectiveDisplayed(500)
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveCompleted(500)
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef && RSVP00_AV_StartedRSVP01
        playerRef.SetValue(RSVP00_AV_StartedRSVP01, 1.0)
    EndIf
    If RSVP01_Quest && !RSVP01_Quest.IsRunning()
        RSVP01_Quest.Start()
    EndIf
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        If RS01A_Contact_Completed
            playerRef.SetValue(RS01A_Contact_Completed, 1.0)
        EndIf
        If pW05_MQ_00P_StartKeyword
            pW05_MQ_00P_StartKeyword.SendStoryEvent(None, playerRef, playerRef)
        EndIf
    EndIf
EndFunction
