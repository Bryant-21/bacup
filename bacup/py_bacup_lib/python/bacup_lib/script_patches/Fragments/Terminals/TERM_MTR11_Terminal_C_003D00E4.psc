Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    Fragments:Quests:QF_MTR11_Delve_002B7C4D questScript = MTR11_Delve as Fragments:Quests:QF_MTR11_Delve_002B7C4D
    If questScript != None && questScript.MTR11_Panel03_Scene != None && !questScript.MTR11_Panel03_Scene.IsPlaying()
        questScript.MTR11_Panel03_Scene.Start()
    EndIf
EndFunction
