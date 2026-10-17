; The Whitespring bellhop protectron trades a claim ticket for the checked item: one MTNL01_ClaimTicket is taken and
; LL_MTNL01_ClaimTicketRewards is handed over.
Function ClaimCheckedItem()
    Actor player = Game.GetPlayer()
    If MTNL01_ClaimTicket == None || player.GetItemCount(MTNL01_ClaimTicket) <= 0
        Return
    EndIf
    player.RemoveItem(MTNL01_ClaimTicket, 1)
    If LL_MTNL01_ClaimTicketRewards != None
        player.AddItem(LL_MTNL01_ClaimTicketRewards, 1)
    EndIf
EndFunction
