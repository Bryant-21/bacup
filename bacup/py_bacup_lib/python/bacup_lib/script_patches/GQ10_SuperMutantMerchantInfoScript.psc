; The super mutant merchant's "Trade!" line opens its barter menu.
Event OnEnd(ObjectReference akSpeakerRef, Bool abHasBeenSaid)
    Actor merchant = akSpeakerRef as Actor
    If merchant != None && !merchant.IsDead()
        merchant.ShowBarterMenu()
    EndIf
EndEvent
