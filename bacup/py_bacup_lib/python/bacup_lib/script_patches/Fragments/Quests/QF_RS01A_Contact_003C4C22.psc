; Stage 0 is the quest's only RunOnStart stage and its record note is "** CHECKPOINT EVAL**".
; FO76 resolved it through DefaultCheckpointingScript, whose CheckpointStages table maps
; checkpointValue 0 (no checkpoint) to StageToSet 10. That script is server-stripped and shared,
; so this fragment carries the single-player equivalent: with no prior progress, open at stage 10.
Function Fragment_Stage_0000_Item_00()
    If !IsStageDone(10)
        SetStage(10)
    EndIf
EndFunction

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
    Keyword questActiveKeyword = Game.GetFormFromFile(0x003B32EC, "SeventySix.esm") as Keyword
    If playerRef && RSVP01_Quest && !RSVP01_Quest.IsRunning() && !RSVP01_Quest.IsCompleted() && questActiveKeyword
        questActiveKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
    EndIf
    If RSVP01_Quest && (RSVP01_Quest.IsRunning() || RSVP01_Quest.IsCompleted()) && !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

; Stage 1100 is the record's "special start up stage that only fires if the player's done
; everything but listen to the Overseer's tape". It is the preReqStage of the alias script
; defaultaliasonplayerholotapeb (watch MQ_Overseer_01A_CAMPHolotape, StageToSet 9000), so it
; must leave the quest in the one state that route consumes: started, with objective 120
; ("Listen to Overseer's Log - C.A.M.P.") open.
Function Fragment_Stage_1100_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef && RS01A_Contact_Started
        playerRef.SetValue(RS01A_Contact_Started, 1.0)
    EndIf
    If !IsObjectiveCompleted(120)
        SetObjectiveDisplayed(120)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        If RS01A_Contact_Completed
            playerRef.SetValue(RS01A_Contact_Completed, 1.0)
        EndIf
    EndIf
    Quest followOn = Game.GetFormFromFile(0x005698E4, "SeventySix.esm") as Quest
    Bool accepted = followOn && (followOn.IsRunning() || followOn.IsCompleted())
    If !accepted && playerRef && pW05_MQ_00P_StartKeyword
        accepted = pW05_MQ_00P_StartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
    EndIf
    If !accepted && followOn
        accepted = followOn.IsRunning() || followOn.IsCompleted()
    EndIf
EndFunction
