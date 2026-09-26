; Alex's parting line in the "Alex opened book" phase of the AlexScene. Stage
; 1020 has him open the trapped book and die; he is protected by the
; Actor_Alex_Essential alias, and his Actor_Alex alias sets 1050 on death.
Function Fragment_End(ObjectReference akSpeakerRef)
    Actor alex = akSpeakerRef as Actor
    If alex != None && !alex.IsDead()
        alex.KillEssential()
    EndIf
EndFunction
