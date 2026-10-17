Event OnBegin(ObjectReference akSpeakerRef, Bool abHasBeenSaid)
    If !GiveOnEnd
        ChargeAndGive()
    EndIf
EndEvent

Event OnEnd(ObjectReference akSpeakerRef, Bool abHasBeenSaid)
    If GiveOnEnd
        ChargeAndGive()
    EndIf
EndEvent

Function ChargeAndGive()
    Actor playerRef = Game.GetPlayer()
    If playerRef == None
        Return
    EndIf
    If BlockingActorValue != None && MaxPurchases > 0 && playerRef.GetValue(BlockingActorValue) >= MaxPurchases as Float
        Return
    EndIf
    ; The line's own GetItemCount(Caps) condition hides it when the player can't pay;
    ; this guards a purchase made while the line was already on screen.
    If PaymentType != None && Price > 0
        If playerRef.GetItemCount(PaymentType) < Price
            Return
        EndIf
        playerRef.RemoveItem(PaymentType, Price)
    EndIf
    If ItemToGive != None && AmounttoGive > 0
        playerRef.AddItem(ItemToGive, AmounttoGive, GiveSilently)
    EndIf
    If BlockingActorValue != None
        playerRef.ModValue(BlockingActorValue, 1.0)
    EndIf
EndFunction
