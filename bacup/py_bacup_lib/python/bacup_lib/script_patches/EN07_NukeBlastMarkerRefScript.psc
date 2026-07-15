Function ClientUpdateMapHazards(Bool isEnabledOnServer)
    If isEnabledOnServer
        If !Self.IsEnabled()
            Self.Enable()
        EndIf
        EN07_NukeMapHazardFormlist.AddForm(Self)
    ElseIf Self.IsEnabled()
        Self.Disable()
    EndIf
EndFunction
