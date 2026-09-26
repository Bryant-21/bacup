Event OnEnd(ObjectReference akSpeakerRef, Bool abHasBeenSaid)
    EN05_PatriotismTrainingQuestScript course = GetOwningQuest() as EN05_PatriotismTrainingQuestScript
    If course == None || !course.IsRunning() || akSpeakerRef == None
        Return
    EndIf
    Form speakerBase = akSpeakerRef.GetBaseObject()
    If speakerBase == EN05_Patriotism_Jimmy_TA
        course.iJimmyHelloValue = (course.iJimmyHelloValue + 1) % 5
    ElseIf speakerBase == EN05_Patriotism_Topher_TA
        course.iTopherHelloValue = (course.iTopherHelloValue + 1) % 5
    ElseIf speakerBase == EN05_Patriotism_Jianjun_TA
        course.iJianjunHelloValue = (course.iJianjunHelloValue + 1) % 5
    EndIf
EndEvent
