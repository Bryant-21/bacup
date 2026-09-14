Event OnQuestInit()
    SelectCurrentInstructor()
EndEvent

ObjectReference Function SelectCurrentInstructor()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf

    ReferenceAlias currentInstructor = GetAlias(172) as ReferenceAlias
    ObjectReference selectedInstructor
    Float selectedDistance
    Int instructorAliasID = 139
    While instructorAliasID <= 171
        ReferenceAlias instructorAlias = GetAlias(instructorAliasID) as ReferenceAlias
        ObjectReference instructorRef
        If instructorAlias != None
            instructorRef = instructorAlias.GetReference()
        EndIf
        If instructorRef != None && playerRef != None && instructorRef.Is3DLoaded()
            Float instructorDistance = playerRef.GetDistance(instructorRef)
            If selectedInstructor == None || instructorDistance < selectedDistance
                selectedInstructor = instructorRef
                selectedDistance = instructorDistance
            EndIf
        EndIf

        If instructorAliasID == 139
            instructorAliasID = 153
        ElseIf instructorAliasID == 153
            instructorAliasID = 171
        Else
            instructorAliasID = 172
        EndIf
    EndWhile

    If currentInstructor != None && selectedInstructor != None
        currentInstructor.ForceRefTo(selectedInstructor)
    EndIf
    Return selectedInstructor
EndFunction
