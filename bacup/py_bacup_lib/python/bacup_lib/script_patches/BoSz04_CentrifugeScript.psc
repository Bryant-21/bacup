Event OnActivate(ObjectReference akActionRef)
    If pBoSZ04 == None
        Return
    EndIf

    Int questStage = pBoSZ04.GetStage()
    If questStage < 95
        If pBoSZ04CentrifugePoweredDownMessage != None
            pBoSZ04CentrifugePoweredDownMessage.Show()
        EndIf
    ElseIf questStage < 200
        If pBoSZ04CentrifugeMissingDNAMessage != None
            pBoSZ04CentrifugeMissingDNAMessage.Show()
        EndIf
    ElseIf questStage < 300
        pBoSZ04.SetStage(300)
    EndIf
EndEvent
