Event OnBegin(ObjectReference akSpeakerRef, Bool abHasBeenSaid)
    If !GiveOnEnd
        GiveItemOnce()
    EndIf
EndEvent

Event OnEnd(ObjectReference akSpeakerRef, Bool abHasBeenSaid)
    If GiveOnEnd
        GiveItemOnce()
    EndIf
EndEvent

; The blocking value is the FO76 per-player "already given" flag; lines that
; share one value share one gift. It is read before the gift so a paired
; DefaultTopicInfoSetActorValue on the same line (attached after this script)
; cannot hide the first hand-over.
Function GiveItemOnce()
    Actor playerRef = Game.GetPlayer()
    If ItemToGive == None || playerRef == None || AmounttoGive <= 0
        Return
    EndIf
    If BlockingActorValue != None && playerRef.GetValue(BlockingActorValue) >= 1.0
        Return
    EndIf
    playerRef.AddItem(ItemToGive, AmounttoGive, GiveSilently)
    If BlockingActorValue != None
        playerRef.SetValue(BlockingActorValue, 1.0)
    EndIf
EndFunction
