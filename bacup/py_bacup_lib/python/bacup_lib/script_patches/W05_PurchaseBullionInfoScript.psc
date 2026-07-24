Event OnBegin(ObjectReference akSpeakerRef, bool abHasBeenSaid)
    If !PurchaseOnBegin
        DoPurchase()
    EndIf
EndEvent

Event OnEnd(ObjectReference akSpeakerRef, bool abHasBeenSaid)
    If PurchaseOnBegin
        DoPurchase()
    EndIf
EndEvent

Function DoPurchase()
    Actor player = Game.GetPlayer()
    If player.GetItemCount(Caps001) < Caps
        Return
    EndIf

    player.RemoveItem(Caps001, Caps, True)

    Int bullionToGrant = 0
    If W05_VendorBullionCost.GetValue() > 0.0
        bullionToGrant = (Caps as Float / W05_VendorBullionCost.GetValue()) as Int
    EndIf
    If bullionToGrant <= 0
        Return
    EndIf

    Int currentBullion = player.GetItemCount(GoldBullion)
    Int overflow = 0
    If currentBullion + bullionToGrant > GoldBullionMax
        overflow = (currentBullion + bullionToGrant) - GoldBullionMax
        bullionToGrant -= overflow
    EndIf

    If bullionToGrant > 0
        player.AddItem(GoldBullion, bullionToGrant, True)
    EndIf

    If overflow > 0
        Int refundCaps = (overflow as Float * W05_VendorBullionCost.GetValue()) as Int
        If refundCaps > 0
            player.AddItem(Caps001, refundCaps, True)
        EndIf
        If GoldBullion_MaxRefundMessage != None
            GoldBullion_MaxRefundMessage.Show()
        EndIf
    EndIf

    player.SetValue(GoldBullion_LastPurchasedTimestamp, Utility.GetCurrentGameTime())
    player.ModValue(W05_BullionPurchased, bullionToGrant as Float)
EndFunction
