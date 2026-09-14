Function RedeemPresidentialSeal(Form akReward)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && EN06_PresidentialSeal != None && akReward != None && playerRef.GetItemCount(EN06_PresidentialSeal) >= 1
        playerRef.RemoveItem(EN06_PresidentialSeal, 1, True)
        playerRef.AddItem(akReward, 1, False)
    EndIf
EndFunction

Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    RedeemPresidentialSeal(PAC_PowerArmor_T60_Presidential_Full)
EndFunction

Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    RedeemPresidentialSeal(GaussRifle_Presidential)
EndFunction

Function Fragment_Terminal_03(ObjectReference akTerminalRef)
    RedeemPresidentialSeal(Clothes_SuitClean_Blue_Presidential)
EndFunction
