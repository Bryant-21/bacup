Function Fragment_Begin(ObjectReference akSpeakerRef)
    ATX_COMP_PetDogScript dog = GetOwningQuest() as ATX_COMP_PetDogScript
    If dog
        dog.GiveDogTreat()
    EndIf
EndFunction
