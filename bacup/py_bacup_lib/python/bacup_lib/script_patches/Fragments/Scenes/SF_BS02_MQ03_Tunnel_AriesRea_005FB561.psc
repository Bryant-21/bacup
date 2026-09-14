Function Fragment_Phase_01_Begin()
    Actor rahmani = actor_Rahmani_Pipeline.GetActorReference()
    ObjectReference doorSpot = xmarker_RahmaniDoorSpot.GetReference()
    If rahmani != None && doorSpot != None
        rahmani.MoveTo(doorSpot)
    EndIf
EndFunction
