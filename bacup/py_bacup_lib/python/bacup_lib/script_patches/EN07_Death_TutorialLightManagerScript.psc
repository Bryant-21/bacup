Function SetTutorialLightStateClient(Int iLightIndex)
    If iLightIndex == -1
        ResetLightState()
        Return
    EndIf
    If ObjectsToToggle == None || iLightIndex < 0 || iLightIndex >= ObjectsToToggle.Length
        Return
    EndIf
    ObjectReference lightRef = ObjectsToToggle[iLightIndex]
    If lightRef != None
        lightRef.Enable(False)
    EndIf
EndFunction

Function ResetLightState()
    If ObjectsToToggle == None
        Return
    EndIf
    Int index = 0
    While index < ObjectsToToggle.Length
        ObjectReference lightRef = ObjectsToToggle[index]
        If lightRef != None
            lightRef.Disable(False)
        EndIf
        index += 1
    EndWhile
EndFunction
