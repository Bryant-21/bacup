Function Fragment_Stage_0200_Item_00()
    Actor playerRef = Game.GetPlayer()
    Book newsletter = Game.GetFormFromFile(0x00529DE1, "SeventySix.esm") as Book
    If playerRef != None && newsletter != None && playerRef.GetItemCount(newsletter) == 0
        playerRef.AddItem(newsletter, 1, False)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(pRSVP00_AV_StartedRSVP01, 1.0)
    EndIf
EndFunction

Function RSVP00_SetPlayerFlag(ActorValue akValue)
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef != None && akValue != None
        playerRef.SetValue(akValue, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    RSVP00_SetPlayerFlag(PRSVP00_AV_gotHolotape_ColonelKid)
EndFunction

Function Fragment_Stage_1100_Item_00()
    RSVP00_SetPlayerFlag(PRSVP00_AV_foundKesha)
EndFunction

Function Fragment_Stage_1200_Item_00()
    RSVP00_SetPlayerFlag(pRSVP00_AV_foundDelbert)
EndFunction

Function Fragment_Stage_1210_Item_00()
    RSVP00_SetPlayerFlag(pRSVP00_AV_foundDelbert)
EndFunction
