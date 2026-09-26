Event OnQuestInit()
    Actor player = Game.GetPlayer()
    If currentPlayer != None && currentPlayer.GetReference() == None && player != None
        currentPlayer.ForceRefTo(player)
    EndIf
    EN07Tutorial_ResetLights()
    EN07Tutorial_SetLight(0)
EndEvent

Function EN07Tutorial_ResetLights()
    If LightManager != None
        ObjectReference managerRef = LightManager.GetReference()
        If managerRef != None
            B21LightManager = managerRef as EN07_Death_TutorialLightManagerScript
        EndIf
    EndIf
    If B21LightManager != None
        B21LightManager.ResetLightState()
    EndIf
EndFunction

Function EN07Tutorial_SetLight(Int aiLightIndex)
    If !IsRunning() || IsStageDone(20)
        Return
    EndIf
    If B21LightManager == None && LightManager != None
        B21LightManager = LightManager.GetReference() as EN07_Death_TutorialLightManagerScript
    EndIf
    If B21LightManager != None
        B21LightManager.SetTutorialLightStateClient(aiLightIndex)
    EndIf
EndFunction

Event OnQuestShutdown()
    EN07Tutorial_ResetLights()
EndEvent
