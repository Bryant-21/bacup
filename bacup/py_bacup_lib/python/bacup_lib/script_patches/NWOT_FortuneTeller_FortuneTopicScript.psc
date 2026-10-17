; The fortune-reading lines (gated on the player lacking the cooldown keyword) pay out the reading when they finish.
Event OnEnd(ObjectReference akSpeakerRef, Bool abHasBeenSaid)
    NWOT_FortuneTeller_DLGQuestScript teller = GetOwningQuest() as NWOT_FortuneTeller_DLGQuestScript
    If teller != None
        teller.GiveFortune()
    EndIf
EndEvent
