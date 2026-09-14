Event OnActivate(ObjectReference akActionRef)
    Debug.Trace("[B21 BoSZ04] Centrifuge activated ref=" + akActionRef as String + " quest=" + pBoSZ04 as String, 0)
    If pBoSZ04 == None
        Debug.Trace("[B21 BoSZ04] Centrifuge has no bound quest", 0)
        Return
    EndIf

    Int questStage = pBoSZ04.GetStage()
    Debug.Trace("[B21 BoSZ04] Centrifuge quest state running=" + pBoSZ04.IsRunning() as String + " stage=" + questStage as String, 0)
    If questStage < 95
        If pBoSZ04CentrifugePoweredDownMessage != None
            pBoSZ04CentrifugePoweredDownMessage.Show()
        EndIf
    ElseIf questStage < 200
        If pBoSZ04CentrifugeMissingDNAMessage != None
            pBoSZ04CentrifugeMissingDNAMessage.Show()
        EndIf
    ElseIf questStage < 300
        Debug.Trace("[B21 BoSZ04] Centrifuge setting stage 300", 0)
        pBoSZ04.SetStage(300)
        Debug.Trace("[B21 BoSZ04] Centrifuge stage result=" + pBoSZ04.GetStage() as String, 0)
    EndIf
EndEvent
